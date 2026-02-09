use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use qdrant_client::qdrant::{SearchPointsBuilder, point_id::PointIdOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;
use crate::services::mapping_service;

#[derive(Deserialize)]
pub struct SuggestQuery {
    pub q: String,
}

#[derive(Serialize)]
pub struct SuggestResponseV2 {
    pub segments: Vec<mapping_service::Segment>,
}

#[derive(Serialize)]
pub struct RootSuggestion {
    pub id: String,
    pub cn_name: String,
    pub en_abbr: String,
    pub score: f32,
}

/// 1. 分词建议接口 (管理员生产标准字段的核心工具)
/// 逻辑：将中文输入利用 JIEBA 切分，并匹配标准词根库（含同义词匹配）
pub async fn suggest_mapping(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SuggestQuery>,
) -> impl IntoResponse {
    let input = query.q.trim();
    if input.is_empty() {
        tracing::warn!("--- 收到空的分词建议请求");
        return (StatusCode::BAD_REQUEST, "查询内容不能为空").into_response();
    }

    tracing::info!(">>> 正在为管理员生成分词建议: q='{}'", input);

    // 调用 Service 层逻辑
    let segments = mapping_service::suggest_field_name(&state.db, input).await;

    (StatusCode::OK, Json(SuggestResponseV2 { segments })).into_response()
}

/// 2. 语义相似度搜索词根 (生产辅助)
pub async fn search_similar_roots(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SuggestQuery>,
) -> impl IntoResponse {
    let input = query.q.trim();
    if input.is_empty() {
        return (StatusCode::BAD_REQUEST, "查询内容不能为空").into_response();
    }

    tracing::info!(">>> 正在检索语义相近词根: q='{}'", input);

    // 步骤 1: 向量化文本。
    // 使用代码块确保 MutexGuard 在向量化完成后立即释放，不阻塞后续异步操作。
    let query_vector_res = {
        let mut model = state.embed_model.lock(); // parking_lot 是同步锁，没有 .await
        model.embed(vec![input], None)
    };

    match query_vector_res {
        Ok(embeddings) => {
            let query_vector = embeddings[0].clone();
            tracing::debug!("--- 向量计算完成，准备检索 Qdrant");

            // 步骤 2: 在 Qdrant 的 word_roots 集合中检索
            let search_res = state
                .qdrant
                .search_points(
                    SearchPointsBuilder::new("word_roots", query_vector, 5).with_payload(true),
                )
                .await;

            match search_res {
                Ok(res) => {
                    let suggestions: Vec<RootSuggestion> = res
                        .result
                        .into_iter()
                        .map(|p| {
                            let pay = p.payload;

                            // 解析 ID
                            let id_str = match p.id {
                                Some(pid) => match pid.point_id_options {
                                    Some(PointIdOptions::Num(n)) => n.to_string(),
                                    Some(PointIdOptions::Uuid(u)) => u,
                                    None => "0".to_string(),
                                },
                                None => "0".to_string(),
                            };

                            // 解析 Payload 字段 (修复类型推断)
                            let cn_name = pay
                                .get("cn_name")
                                .and_then(|v| v.as_str())
                                .map(|s| s.as_str()) // 显式转换为 &str
                                .unwrap_or("")
                                .to_string();

                            let en_abbr = pay
                                .get("en_abbr")
                                .and_then(|v| v.as_str())
                                .map(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();

                            RootSuggestion {
                                id: id_str,
                                cn_name,
                                en_abbr,
                                score: p.score,
                            }
                        })
                        .collect();

                    tracing::info!("<<< 语义搜索完成: 召回数量={}", suggestions.len());
                    (StatusCode::OK, Json(suggestions)).into_response()
                }
                Err(e) => {
                    tracing::error!("!!! Qdrant 检索词根异常: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("向量库检索失败: {}", e),
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("!!! 向量模型计算异常: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("向量计算失败: {}", e),
            )
                .into_response()
        }
    }
}
