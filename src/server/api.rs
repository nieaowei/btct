use anyhow::anyhow;
use axum::Json;
use reqwest::StatusCode;

use crate::server::{
    dto::{GetSnipePrams, GetSnipeResp},
    model::{Code, Response},
};

type ApiResult<T> = Result<Response<T>, Response<T>>;

pub(super) async fn get_snipe(Json(params): Json<GetSnipePrams>) -> ApiResult<GetSnipeResp> {
    Ok(Response::success(GetSnipeResp {
        hex: "".to_string(),
    }))
}

pub(super) async fn post_snipe(Json(params): Json<GetSnipePrams>) -> ApiResult<String> {
    Err(anyhow!("anyhow error")).map_err(|e| (e, Code::MissingParam))?;
    Ok(Response::success(params.txid))
}
