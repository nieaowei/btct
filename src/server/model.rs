use axum::{http::StatusCode, response, response::IntoResponse, Json};
use serde::Serialize;
use serde_repr::Serialize_repr;

#[derive(Serialize_repr)]
#[repr(u16)]
pub(super) enum Code {
    Success = 0,
    Unknown = 1,
    MissingParam = 2,
    MisdirectParam = 3,
}

#[derive(Serialize)]
pub(super) struct Response<T>
where
    T: Serialize,
{
    code: Code,
    msg: String,
    data: Option<T>,
    #[serde(skip)]
    _status_code: StatusCode,
}

pub(super) enum Error {
    Unknown(anyhow::Error),
    // MissingParam(),
}

impl<T> Response<T>
where
    T: Serialize,
{
    pub(super) fn success(data: T) -> Response<T> {
        Self {
            code: Code::Success,
            msg: "".to_string(),
            data: Some(data),
            _status_code: StatusCode::OK,
        }
    }
    pub(super) fn code(self, code: Code) -> Response<T> {
        Self {
            code,
            msg: self.msg,
            data: self.data,
            _status_code: self._status_code,
        }
    }
    pub(super) fn status(self, status_code: StatusCode) -> Response<T> {
        Self {
            code: self.code,
            msg: self.msg,
            data: self.data,
            _status_code: status_code,
        }
    }
    pub(super) fn msg(self, msg: &str) -> Response<T> {
        Self {
            code: self.code,
            msg: msg.to_string(),
            data: self.data,
            _status_code: self._status_code,
        }
    }
    pub(super) fn data(self, data: T) -> Response<T> {
        Self {
            code: self.code,
            msg: self.msg,
            data: Some(data),
            _status_code: self._status_code,
        }
    }
}

impl<T> IntoResponse for Response<T>
where
    T: Serialize,
{
    fn into_response(self) -> response::Response {
        (self._status_code, Json(self)).into_response()
    }
}

impl<T> From<anyhow::Error> for Response<T>
where
    T: Serialize,
{
    fn from(value: anyhow::Error) -> Self {
        Self {
            code: Code::Unknown,
            msg: value.to_string(),
            data: None,
            _status_code: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl<T, E> From<(E, Code)> for Response<T>
where
    T: Serialize,
    E: ToString,
{
    fn from(value: (E, Code)) -> Self {
        Self {
            code: value.1,
            msg: value.0.to_string(),
            data: None,
            _status_code: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl<T, E> From<(E, Code, StatusCode)> for Response<T>
where
    T: Serialize,
    E: ToString,
{
    fn from(value: (E, Code, StatusCode)) -> Self {
        Self {
            code: value.1,
            msg: value.0.to_string(),
            data: None,
            _status_code: value.2,
        }
    }
}
//
// impl IntoResponse for Error {
//     fn into_response(self) -> response::Response {
//         match self {
//             Error::Unknown(err) => {}
//         }
//     }
// }
