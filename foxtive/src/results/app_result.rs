use crate::results::AppResult;

pub trait IntoAppResult<T> {
    fn into_app_result(self) -> AppResult<T>;
}

#[cfg(feature = "rabbitmq")]
impl<T> IntoAppResult<T> for crate::rabbitmq::RmqResult<T> {
    fn into_app_result(self) -> AppResult<T> {
        self.map_err(|e| crate::prelude::AppMessage::Infrastructure {
            message: format!("RabbitMQ error: {e}"),
            source: Some(Box::new(e)),
        })
    }
}

#[cfg(any(feature = "database", feature = "database-async"))]
impl<T> IntoAppResult<T> for diesel::QueryResult<T> {
    fn into_app_result(self) -> AppResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(diesel::result::Error::NotFound) => {
                Err(crate::prelude::AppMessage::not_found("Resource not found"))
            }
            Err(e) => Err(e.into()),
        }
    }
}
