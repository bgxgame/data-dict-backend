use axum::{
    routing::{get, post, put},
    Router,
};
use dotenvy::dotenv;
use jieba_rs::Jieba;
use once_cell::sync::Lazy;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// 声明子模块
mod handlers;
mod middleware;
mod models;
mod services;

// 使用 Lazy 确保 Jieba 词库只在启动时加载一次，并全局可用
pub static JIEBA: Lazy<Jieba> = Lazy::new(Jieba::new);

// 定义全局状态，方便在 Handler 中获取数据库连接池
pub struct AppState {
    pub db: PgPool,
}

#[tokio::main]
async fn main() {
    // 1. 初始化日志系统
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. 加载 .env 环境变量
    dotenv().ok();
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file");

    // 3. 初始化数据库连接池
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create database connection pool");

    let shared_state = Arc::new(AppState { db: pool });

    // 4. 配置跨域 (CORS) - 开发阶段允许所有，生产环境需收紧
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 5. 构建路由
    // 1. 认证路由 (公开)
    let auth_routes = Router::new()
        .route("/signup", post(handlers::auth_handler::signup))
        .route("/login", post(handlers::auth_handler::login));

    // 2. 用户查询路由 (公开)
    let public_routes = Router::new().route("/search", get(handlers::field_handler::search_field));

    // 3. 管理员路由 (受保护)
    let admin_routes = Router::new()
        .route(
            "/roots",
            post(handlers::word_root_handler::create_root)
                .get(handlers::word_root_handler::list_roots),
        )
        .route(
            "/roots/:id",
            put(handlers::word_root_handler::update_root)
                .delete(handlers::word_root_handler::delete_root),
        )
        .route(
            "/fields",
            post(handlers::field_handler::create_field).get(handlers::field_handler::list_fields),
        )
        .route(
            "/fields/:id",
            get(handlers::field_handler::get_field_details)
                .put(handlers::field_handler::update_field)
                .delete(handlers::field_handler::delete_field),
        )
        // 修复：建议接口属于管理员生产工具，移入 admin
        .route("/suggest", get(handlers::mapping_handler::suggest_mapping))
        .layer(axum::middleware::from_fn_with_state(
            shared_state.clone(),
            middleware::auth::guard,
        ));

    let app = Router::new()
        .nest("/api/auth", auth_routes)
        .nest("/api/public", public_routes)
        .nest("/api/admin", admin_routes)
        .with_state(shared_state)
        .layer(cors);
    // 6. 启动服务
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("🚀 Server started at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
