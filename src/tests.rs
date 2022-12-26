use crate::*;
use config::*;

fn get_user_data_json() -> String {
    let path = Path::new(USER_DATA_PATH);

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
