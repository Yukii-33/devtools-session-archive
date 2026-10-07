mod session_archive;

use axum::{extract::State, routing::post, Json, Router};
use session_archive::{Archive, ArchiveError, SessionRequest, SessionTicket};

async fn open_session(State(archive): State<Archive>, Json(input): Json<SessionRequest>) -> Result<Json<SessionTicket>, ArchiveError> {
    Ok(Json(archive.open(input).await?))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("INFRAI_API_KEY")?;
    let bucket = std::env::var("ARCHIVE_BUCKET")?;
    let archive = Archive::new(key, bucket);
    archive.create_bucket().await?;
    let app = Router::new().route("/sessions", post(open_session)).with_state(archive);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
