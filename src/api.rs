use crate::structs::*;

use std::{
    error::Error
};
use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};

//////////////////////////////////////////////////////////
// API calls
//////////////////////////////////////////////////////////
pub async fn fetch_geocode(
    addr: String,
    city: String,
) -> Result<(String, String), Box<dyn Error + Send + Sync>> {
    let api_token = std::env::var("LOCATIONIQ_TOKEN").expect("LOCATIONIQ_TOKEN must be set.");

    let url =
        format!("https://eu1.locationiq.com/v1/search?key={}&q={}, {}&format=json", api_token, addr, city);

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "reqwest/0.11.13")
        .send()
        .await?
        .text()
        .await?;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
        let lat = &json[0]["lat"].as_str().unwrap();
        let lon = &json[0]["lon"].as_str().unwrap();
        return Ok((lat.to_string(), lon.to_string()));
    }
    Err("Fetching geocode!")?
}

pub async fn fetch_address(lat: String, lon: String) -> Result<String, Box<dyn Error + Send + Sync>> {
    let api_token = std::env::var("LOCATIONIQ_TOKEN").expect("LOCATIONIQ_TOKEN must be set.");
    let url =
        format!("https://eu1.locationiq.com/v1/reverse?key={}&lat={}&lon={}&format=json", api_token, lat, lon);

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "reqwest/0.11.13")
        .send()
        .await?
        .text()
        .await?;

    let json: serde_json::Value = serde_json::from_str(&resp).unwrap();

    let street = json["address"]["road"].as_str().unwrap();
    let mut res = format!("{}", street);
    if let Some(house_number) = json["address"]["house_number"].as_str() {
        res = format!("{} {}", res, house_number);
    };
    let _city = json["address"]["city"].as_str().unwrap();
    
    Ok(res)
}

pub async fn get_nearby_stations(
    lat: String,
    lon: String,
) -> Result<Vec<Station>, Box<dyn Error + Send + Sync>> {
    let url = format!("https://v5.db.transport.rest/stops/nearby?latitude={}&longitude={}", lat, lon);

    let mut stations_value: Vec<serde_json::Value> = vec![];

    let response = reqwest::get(url).await?.text().await?;
    let json: serde_json::Value = serde_json::from_str(&response).unwrap();

    // Change using iterators
    let mut i = 0;
    while json[i].is_object() {
        stations_value.push(json[i].to_owned());
        i += 1;
    }

    let mut stations: Vec<Station> = vec![];
    for s in stations_value.iter() {
        let station_name = s.get("name").unwrap().as_str().unwrap().to_string();
        let station_name = station_name.splitn(2, ",").next().unwrap().to_string();
        let station = Station {
            id: s.get("id").unwrap().as_str().unwrap().to_string(),
            name: station_name,
            location: Location {
                lat: s
                    .get("location")
                    .unwrap()
                    .get("latitude")
                    .unwrap()
                    .to_string(),
                lon: s
                    .get("location")
                    .unwrap()
                    .get("longitude")
                    .unwrap()
                    .to_string(),
            },
            distance: s.get("distance").unwrap().as_i64().unwrap(),
        };
        stations.push(station);
    }

    Ok(stations)
}

pub async fn get_departures(
    stop_id: String,
) -> Result<Vec<TransitDeparture>, Box<dyn Error + Send + Sync>> {
    let url = format!(
        "https://v5.db.transport.rest/stops/{}/departures",
        // "https://v5.db.transport.rest/stops/{}/departures?when=today 9am", // for debugging
        stop_id
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "reqwest/0.11.13")
        .send()
        .await?
        .text()
        .await?;

    let mut departures_values: Vec<serde_json::Value> = vec![];

    let json: serde_json::Value = serde_json::from_str(&resp).unwrap();

    // Change using iterators
    let mut i = 0;
    while json[i].is_object() {
        departures_values.push(json[i].to_owned());
        i += 1;
    }

    let mut transits: Vec<TransitDeparture> = vec![];
    for d in departures_values.iter() {
        let transit_name = d
            .get("line")
            .unwrap()
            .get("name")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let transit_planned = d.get("plannedWhen").unwrap().as_str().unwrap().to_string();
        let transit_delay = match d.get("delay") {
            Some(i) => i.as_i64(),
            None => None,
        };
        let transit_direction = d.get("direction").unwrap().as_str().unwrap().to_string();

        let transit_destination = Station {
            id: d
                .get("destination")
                .unwrap()
                .get("id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            name: d
                .get("destination")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            location: Location {
                lat: d
                    .get("destination")
                    .unwrap()
                    .get("location")
                    .unwrap()
                    .get("latitude")
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_string(),
                lon: d
                    .get("destination")
                    .unwrap()
                    .get("location")
                    .unwrap()
                    .get("longitude")
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_string(),
            },
            distance: -1, // -1 means undefined
        };

        let transit_curr_position = match d.get("currentTripPosition") {
            Some(v) => Some(Location {
                lat: v.get("latitude").unwrap().as_f64().unwrap().to_string(),
                lon: v.get("longitude").unwrap().as_f64().unwrap().to_string(),
            }),
            None => None,
        };

        let transit = TransitDeparture {
            stop_id: stop_id.clone(),
            planned: transit_planned,
            delay: transit_delay,
            direction: transit_direction,
            name: transit_name,
            destination: transit_destination,
            curr_position: transit_curr_position,
        };
        transits.push(transit);
    }
    Ok(transits)
}