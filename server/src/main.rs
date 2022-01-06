use rocket::{launch, routes, get, State};
use rocket::serde::{json::Json};
use serde::{Serialize, Deserialize};
use chrono::prelude::*;
use std::sync::{Arc, RwLock};
use serde_json::Value;
use anyhow::Context;

#[derive(Debug)]
struct Forecast {
    timestamp: u64,
    temperature: f32,
    humidity: f32,
    condition_code: u16
}

#[derive(Debug)]
struct DailyForecast {
    timestamp: u64,
    temperature: f32,
    temp_min: f32,
    temp_max: f32,
    humidity: f32,
    condition_code: u16
}

impl Forecast {
    fn empty() -> Forecast {
        Forecast {
            timestamp: 0,
            temperature: 0.0,
            humidity: 0.0,
            condition_code: 0
        }
    }

    fn write_to(&self, out: &mut impl std::io::Write) {
        use byteorder::{LE, WriteBytesExt};
        out.write_f32::<LE>(self.temperature);
        out.write_f32::<LE>(self.humidity);
        out.write_u16::<LE>(self.condition_code);
    }
}

impl DailyForecast {
    fn write_to(&self, out: &mut impl std::io::Write) {
        use byteorder::{LE, WriteBytesExt};
        out.write_f32::<LE>(self.temperature);
        out.write_f32::<LE>(self.humidity);
        out.write_f32::<LE>(self.temp_min);
        out.write_f32::<LE>(self.temp_max);
        out.write_u16::<LE>(self.condition_code);
    }
}

#[derive(Debug)]
struct WeatherInfo {
    current: Forecast,
    hourly: Vec<Forecast>,
    daily: Vec<DailyForecast>
}

impl WeatherInfo {
    fn empty() -> WeatherInfo {
        WeatherInfo {
            current: Forecast::empty(),
            hourly: Vec::new(),
            daily: Vec::new()
        }
    }
}

type WeatherInfoS = Arc<RwLock<WeatherInfo>>;

#[get("/wu")]
fn watch_update(current_weather: &State<WeatherInfoS>) -> String {
    use std::io::Write;
    use byteorder::{LE, WriteBytesExt};
    let now = Utc::now().timestamp() as u64;
    let wi = current_weather.read().unwrap();
    let mut data = Vec::new();
    // output current data
    wi.current.write_to(&mut data);
    // output next 4 hours
    for h in wi.hourly.iter().skip_while(|h| h.timestamp < now).take(4) {
        h.write_to(&mut data);
    }
    // output daily weather
    for d in wi.daily.iter() {
        d.write_to(&mut data);
    }
    unsafe { String::from_utf8_unchecked(data) }
}

#[launch]
fn rocket() -> _ {
    let weather_info = Arc::new(RwLock::new(WeatherInfo::empty()));
    {
        let wi = weather_info.clone();
        std::thread::spawn(move || {
            let (lat, lon) = (40.2338, -111.6585);
            let api_key = std::env::var("OPENWEATHERAPI_KEY").unwrap();
            let current_req_url = format!("http://api.openweathermap.org/data/2.5/weather?lat={lat}&lon={lon}&appid={api_key}&units=metric");
            let full_req_url = format!("http://api.openweathermap.org/data/2.5/onecall?lat={lat}&lon={lon}&appid={api_key}&units=metric&exclude=minutely,alerts");
            let mut full_refresh_counter = 9999;
            loop {
                full_refresh_counter += 1;
                if full_refresh_counter >= 6 /* one hour */ {
                    full_refresh_counter = 0;
                    match ureq::get(&full_req_url)
                        .call().context("get full weather")
                        .and_then(|r| r.into_json::<Value>().context("parse weather result"))
                    {
                        Ok(resp) => {
                            let mut ww = wi.write().unwrap();
                            ww.current.timestamp   = resp["current"]["dt"].as_u64().unwrap_or(0);
                            ww.current.temperature = resp["current"]["temp"].as_f64().unwrap_or(-999.0) as f32;
                            ww.current.humidity    = resp["current"]["humidity"].as_f64().unwrap_or(-999.0) as f32;
                            ww.current.condition_code = resp["current"]["weather"][0]["id"].as_u64().unwrap_or(0) as u16;
                            ww.hourly = resp["hourly"].as_array().unwrap().iter()
                                .map(|h| Forecast {
                                    timestamp: h["dt"].as_u64().unwrap_or(0),
                                    temperature: h["temp"].as_f64().unwrap_or(-999.0) as f32,
                                    humidity: h["humidity"].as_f64().unwrap_or(-999.0) as f32,
                                    condition_code: h["weather"][0]["id"].as_u64().unwrap_or(0) as u16
                                })
                                .collect();
                            ww.daily = resp["daily"].as_array().unwrap().iter()
                                .map(|h| DailyForecast {
                                    timestamp: h["dt"].as_u64().unwrap_or(0),
                                    temperature: h["temp"]["day"].as_f64().unwrap_or(-999.0) as f32,
                                    humidity: h["humidity"].as_f64().unwrap_or(-999.0) as f32,
                                    temp_min: h["temp"]["min"].as_f64().unwrap_or(-999.0) as f32,
                                    temp_max: h["temp"]["max"].as_f64().unwrap_or(-999.0) as f32,
                                    condition_code: h["weather"][0]["id"].as_u64().unwrap_or(0) as u16
                                })
                                .collect();
                        },
                        Err(e) => {
                            println!("error getting current weather: {}", e);
                        }
                    }
                } else {
                    match ureq::get(&current_req_url)
                        .call().context("get current weather")
                        .and_then(|r| r.into_json::<Value>().context("parse weather result"))
                    {
                        Ok(resp) => {
                            let mut ww = wi.write().unwrap();
                            ww.current.timestamp   = resp["dt"].as_u64().unwrap();
                            ww.current.temperature = resp["main"]["temp"].as_f64().unwrap_or(-999.0) as f32;
                            ww.current.humidity    = resp["main"]["humidity"].as_f64().unwrap_or(-999.0) as f32;
                            ww.current.condition_code = resp["weather"][0]["id"].as_u64().unwrap_or(0) as u16;
                        },
                        Err(e) => {
                            println!("error getting current weather: {}", e);
                        }
                    }
                }
                {
                    println!("cw={:#?}", wi.read().unwrap());
                }
                std::thread::sleep(std::time::Duration::from_secs(600 /* 10 minutes */));
            }
        });
    }
    rocket::build()
        .manage(weather_info)
        .mount("/", routes![watch_update])
}
