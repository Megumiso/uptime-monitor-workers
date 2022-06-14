use std::time::Duration;
use chrono::{NaiveDateTime, Utc};
use serde_json::json;
use worker::*;
use serde::{Deserialize, Serialize};

mod utils;

#[derive(Serialize, Deserialize)]
struct Uptime {
    timestamp: i64,
    result: String,
    ping: i64,
}

#[derive(Serialize, Deserialize)]
struct UptimeVec {
    uptime: Vec<Uptime>
}

fn log_request(req: &Request) {
    console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or("unknown region".into())
    );
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    log_request(&req);

    // Optionally, get more helpful error messages written to the console in the case of a panic.
    utils::set_panic_hook();

    // Optionally, use the Router to handle matching endpoints, use ":name" placeholders, or "*name"
    // catch-alls to match on specific patterns. Alternatively, use `Router::with_data(D)` to
    // provide arbitrary data that will be accessible in each route via the `ctx.data()` method.
    let router = Router::new();

    // Add as many routes as your Worker needs! Each route will get a `Request` for handling HTTP
    // functionality and a `RouteContext` which you can use to  and get route parameters and
    // Environment bindings like KV Stores, Durable Objects, Secrets, and Variables.
    router
        .get_async("/", |mut req, ctx| async move {
            let kv = match ctx.kv("megumiso-uptime") {
                Ok(i) => {
                    i
                },
                Err(e) => {
                    console_log!("Error {:?}", e);
                    panic!("");
                }
            };

            let kv_text = kv.get("result").text().await.unwrap().unwrap();
            let mut uptime_vec:Vec<Uptime> = serde_json::from_str(&kv_text).unwrap();

            Response::from_json(&uptime_vec)
        })
        .post_async("/form/:field", |mut req, ctx| async move {
            if let Some(name) = ctx.param("field") {
                let form = req.form_data().await?;
                match form.get(name) {
                    Some(FormEntry::Field(value)) => {
                        return Response::from_json(&json!({ name: value }))
                    }
                    Some(FormEntry::File(_)) => {
                        return Response::error("`field` param in form shouldn't be a File", 422);
                    }
                    None => return Response::error("Bad Request", 400),
                }
            }

            Response::error("Bad Request", 400)
        })
        .run(req, env)
        .await
}

#[event(scheduled)]
pub async fn schedule(event: ScheduledEvent, env: worker::Env, ctx: ScheduleContext){

    console_log!("cron ok! v5");
    let kv = match env.kv("megumiso-uptime") {
        Ok(i) => {
            i
        },
        Err(e) => {
            console_log!("Error {:?}", e);
            panic!("");
        }
    };

    let current_time = chrono::offset::Utc::now();
    let time_i64 = current_time.timestamp();

    let before_ping = chrono::offset::Utc::now().timestamp_millis();
    let result = reqwest::get(env.var("PING_URL").unwrap().to_string()).await;
    let after_ping = chrono::offset::Utc::now().timestamp_millis();

    let ping_time = after_ping - before_ping;

    let ping_result = match result {
        Ok(i) => {
            "Success"
        },
        Err(e) => {
            "No Response"
        }
    };

    let uptime = Uptime {
        timestamp: time_i64,
        result: ping_result.to_string(),
        ping: ping_time
    };

    let kv_text = kv.get("result").text().await.unwrap().unwrap();

    let mut uptime_vec:Vec<Uptime> = serde_json::from_str(&kv_text).unwrap();

    uptime_vec.insert(0,uptime);

    let uptime_json = serde_json::to_string(&uptime_vec).unwrap();

    kv.put("result", uptime_json).unwrap().execute().await.unwrap();
}
