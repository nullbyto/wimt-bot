use crate::*;

fn get_user_data_json() -> String {
    let user_data_path = std::env::var("USER_DATA_PATH").expect("USER_DATA_PATH must be set.");
    let path = Path::new(&user_data_path);

    let mut file_data = String::new();
    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path)
        .expect("Unable to open file");
    file.read_to_string(&mut file_data).expect("Unable to read to string");

    let json = serde_json::to_string(&file_data).unwrap();
    json
}

#[test]
fn test_storing_data() {
    let user1 = UserData::default();
    let user2 = UserData::default();
    
    let _ = store_user_data(user1);
    let json1 = get_user_data_json();
    let _ = store_user_data(user2);
    let json2 = get_user_data_json();

    assert_ne!(json1, json2);
}

#[tokio::test]
async fn test_fetch_geocode() {
    use dotenv::dotenv;
    dotenv().ok();
    let geocode = api::fetch_geocode("empire state building".into(), "nyc".into()).await.unwrap();
    assert_eq!(geocode, ("40.748428399999995".to_string(), "-73.98565461987332".to_string()));
}

#[tokio::test]
async fn test_fetch_address() {
    use dotenv::dotenv;
    dotenv().ok();
    let addr = api::fetch_address("40.748428399999995".into(), "-73.98565461987332".into()).await.unwrap();
    assert_eq!(addr, "5th Avenue 350");
}