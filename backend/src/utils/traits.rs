pub trait OptionExt<T, R>
where
    R: warp::reject::Reject,
{
    fn or_reject(self, rejection: R) -> Result<T, warp::Rejection>;
    fn or_reject_and_log(self, rejection: R, err: &str) -> Result<T, warp::Rejection>;
}

pub trait ResultExt<T, E, R>
where
    R: warp::reject::Reject,
{
    fn or_reject(self, rejection: R) -> Result<T, warp::Rejection>;
    fn or_reject_and_log(self, rejection: R, err: &str) -> Result<T, warp::Rejection>;
}

impl<T, E, R> ResultExt<T, E, R> for Result<T, E>
where
    R: warp::reject::Reject,
    E: std::fmt::Debug,
{
    fn or_reject(self, rejection: R) -> Result<T, warp::Rejection> {
        match self {
            Ok(v) => Ok(v),
            Err(_) => Err(warp::reject::custom(rejection)),
        }
    }

    fn or_reject_and_log(self, rejection: R, err: &str) -> Result<T, warp::Rejection> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => {
                tracing::error!("{}: {:?}", err, e);
                Err(warp::reject::custom(rejection))
            }
        }
    }
}

impl<T, R> OptionExt<T, R> for Option<T>
where
    R: warp::reject::Reject,
{
    fn or_reject(self, rejection: R) -> Result<T, warp::Rejection> {
        match self {
            Some(v) => Ok(v),
            None => Err(warp::reject::custom(rejection)),
        }
    }

    fn or_reject_and_log(self, rejection: R, err: &str) -> Result<T, warp::Rejection> {
        match self {
            Some(v) => Ok(v),
            None => {
                tracing::error!("{}", err);
                Err(warp::reject::custom(rejection))
            }
        }
    }
}
