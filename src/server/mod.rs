use axum::{
    routing::{get, post},
    Json, Router, ServiceExt,
};
use tokio::net::TcpListener;

mod api;
mod error;
mod mw;

mod dto;
mod model;

pub(crate) fn start() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    // rt.block_on(async {
    //     let r = Router::new()
    //         .route("/snipe", post(api::snipe))
    //         .route("/cancel_tx", post(api::cancel_tx));
    //     let listener = TcpListener::bind("0.0.0.0:9091").await.unwrap();
    //     axum::serve(listener, r).await.unwrap();
    // });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sever() {
        start().expect("");
    }
}
