use crate::{AppState, errors::AppError};
use axum::extract::ConnectInfo;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use redis::{Script, aio::MultiplexedConnection};
use std::net::SocketAddr;
use std::sync::Arc;

// this is for pracrtice,
// uses sliding log rate limiter
// could also use token bucket or leaky bucket, but sliding log is more accurate and fair
pub async fn ip_rate_limiter_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // in production it would be x-forwarded-for
    // we dont have trusted proxies here
    // get the ip address of the request
    let ip = addr.ip().to_string();

    // check if the ip is allowed to make a request
    if !is_allowed(&ip, state.redis_conn.clone()).await {
        return Err(AppError::TooManyRequests);
    }

    Ok(next.run(req).await)
}

pub async fn business_rate_limiter_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // get the business id from the request extensions
    let business_id = req
        .extensions()
        .get::<crate::middleware::auth::AuthenticatedBusiness>()
        .map(|b| b.business.id.to_string())
        .unwrap_or("unknown".to_string());

    // check if the business is allowed to make a request
    if !is_allowed(&business_id, state.redis_conn.clone()).await {
        return Err(AppError::TooManyRequests);
    }

    Ok(next.run(req).await)
}

const WINDOW_MS: u64 = 60_000; // 60 seconds
const MAX_REQUESTS: u64 = 100;

const SLIDING_LOG_SCRIPT: &str = r#"
local key      = KEYS[1]
local now      = tonumber(ARGV[1])
local window   = tonumber(ARGV[2])
local max_req  = tonumber(ARGV[3])
local member   = ARGV[4]
local cutoff   = now - window

-- remove entries of a key that are older than the window 
redis.call('ZREMRANGEBYSCORE', key, 0, cutoff)

local count = redis.call('ZCARD', key)

if count >= max_req then
    return 0
end

redis.call('ZADD', key, now, member)
-- expire the whole key if  untouched for a while
redis.call('PEXPIRE', key, window + 1000)

return 1
"#;

async fn is_allowed(key: &str, mut redis: MultiplexedConnection) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // unique member to avoid same-ms collisions
    let member = format!("{}-{}", now, uuid::Uuid::new_v4());

    let result: redis::RedisResult<i64> = Script::new(SLIDING_LOG_SCRIPT)
        .key(key)
        .arg(now)
        .arg(WINDOW_MS)
        .arg(MAX_REQUESTS)
        .arg(&member)
        .invoke_async(&mut redis)
        .await;

    match result {
        Ok(1) => true,
        Ok(_) => false,
        Err(e) => {
            // fail open, don't block requests because Redis hiccuped
            tracing::warn!("Redis rate limiter error, failing open: {}", e);
            true
        }
    }
}
