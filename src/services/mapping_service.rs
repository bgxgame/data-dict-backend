use sqlx::PgPool;
use crate::models::word_root::WordRoot;
use serde::Serialize;

#[derive(Serialize)]
pub struct Segment {
    pub word: String,              // 原始切分的词
    pub candidates: Vec<WordRoot>, // 匹配到的所有候选词根（包含名称匹配和同义词匹配）
}

pub async fn suggest_field_name(pool: &PgPool, cn_input: &str) -> Vec<Segment> {
    let input = cn_input.trim();
    if input.is_empty() { return vec![]; }

    // --- 阶段 1：全称精准/同义词匹配 ---
    // 逻辑：如果不拆分就能匹配到词根，说明这是一个完整的业务术语，优先保留。
    let full_pattern = format!("%{}%", input);
    let full_candidates: Vec<WordRoot> = sqlx::query_as!(
        WordRoot,
        r#"SELECT id, cn_name, en_abbr, en_full_name, associated_terms, remark, created_at 
           FROM standard_word_roots 
           WHERE cn_name = $1 
           OR associated_terms ILIKE $2
           ORDER BY (cn_name = $1) DESC, cn_name ASC"#,
        input,
        full_pattern
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // 如果全称匹配到了结果，直接返回单段结果，不再切分
    if !full_candidates.is_empty() {
        tracing::info!("全称匹配成功: {}", input);
        return vec![Segment {
            word: input.to_string(),
            candidates: full_candidates,
        }];
    }

    // --- 阶段 2：分词匹配逻辑 ---
    // 逻辑：全称没搜到，说明需要拆分组合。
    tracing::info!("全称未命中，进入分词逻辑: {}", input);
    
    // 获取读锁
    let jieba_read = crate::JIEBA.read().await;
    // 使用精准模式切分中文
    let words = jieba_read.cut(input, false);
    
    let mut segments = Vec::new();

    for word in words {
        let trimmed = word.trim();
        if trimmed.is_empty() { continue; }
        
        let pattern = format!("%{}%", trimmed);

        let candidates: Vec<WordRoot> = sqlx::query_as!(
            WordRoot,
            r#"SELECT id, cn_name, en_abbr, en_full_name, associated_terms, remark, created_at 
               FROM standard_word_roots 
               WHERE cn_name = $1 
               OR associated_terms ILIKE $2
               ORDER BY (cn_name = $1) DESC, cn_name ASC"#,
            trimmed,
            pattern
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        segments.push(Segment {
            word: trimmed.to_string(),
            candidates,
        });
    }
    segments
}