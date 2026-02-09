use sqlx::PgPool;
use crate::models::word_root::WordRoot;
use serde::Serialize;

#[derive(Serialize)]
pub struct Segment {
    pub word: String,              // 原始切分的词
    pub candidates: Vec<WordRoot>, // 匹配到的所有候选词根（包含名称匹配和同义词匹配）
}

pub async fn suggest_field_name(pool: &PgPool, cn_input: &str) -> Vec<Segment> {
    // 获取读锁
    let jieba_read = crate::JIEBA.read().await;
    // 使用精准模式切分中文
    let words = jieba_read.cut(cn_input, false);
    
    let mut segments = Vec::new();

    for word in words {
        let trimmed = word.trim();
        if trimmed.is_empty() { continue; }
        
        // 模糊匹配模式：前后加 % 
        let pattern = format!("%{}%", trimmed);

        // 显式指定类型 Vec<WordRoot> 
        // 优化 SQL：
        // 1. 同时查询名称和同义词 (associated_terms)
        // 2. 增加排序逻辑：名称完全匹配的排在最前面，其次按名称长度排序
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