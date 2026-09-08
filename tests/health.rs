use axum::http::StatusCode;
use pando::app::web::health::health_check;

#[tokio::test]
async fn health_check_is_ok_without_touching_the_database() {
    assert_eq!(health_check().await, StatusCode::OK);
}
