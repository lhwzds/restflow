use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn ok_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            message: Some(message.into()),
        }
    }
}

impl ApiResponse<()> {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(()),
            message: Some(message.into()),
        }
    }
}

impl<T: Serialize, E: std::error::Error> From<E> for ApiResponse<T> {
    fn from(error: E) -> Self {
        ApiResponse::error(error.to_string())
    }
}
