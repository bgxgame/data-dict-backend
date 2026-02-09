use sqlx::PgPool;
use crate::models::word_root::WordRoot;
use serde::Serialize;

#[derive(Serialize)]
pub struct Segment {
    pub word: String,              // 原始切分的词
    pub candidates: Vec<WordRoot>, // 匹配到的所有候选词根
}


pub async fn suggest_field_name(pool: &PgPool, cn_input: &str) -> Vec<Segment> {
    let jieba_read = crate::JIEBA.read().await;
    let words = jieba_read.cut(cn_input, false);
    
    let mut segments = Vec::new();

    for word in words {
        let trimmed = word.trim();
        if trimmed.is_empty() { continue; }
        
        // 优化点：去掉 LIMIT 1，获取所有匹配的词根
        let candidates = sqlx::query_as!(
            WordRoot,
            r#"SELECT * FROM standard_word_roots 
               WHERE cn_name = $1 
               OR associated_terms ~* $2"#,
            trimmed,
            format!(r"(^|[[:space:]]){}([[:space:]]|$)", trimmed)
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