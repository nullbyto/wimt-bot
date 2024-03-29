pub mod api;
pub mod io;
pub mod structs;
#[cfg(test)]
mod tests;

use api::*;
use chrono::{DateTime, Duration};
use io::*;
use structs::*;

use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
    vec,
};
use dotenv::dotenv;
use dptree::{case, deps};
use teloxide::{
    dispatching::{dialogue, dialogue::InMemStorage},
    dptree::endpoint,
    filter_command,
    payloads::SendMessageSetters,
    prelude::*,
    types::{
        InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, KeyboardRemove,
        MessageCommon, MessageKind, ParseMode::Html, ReplyMarkup,
    },
    utils::command::BotCommands,
};
use tokio::task::JoinHandle;

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
type MyDialogue = Dialogue<State, InMemStorage<State>>;
type MyTasksMap = Arc<Mutex<HashMap<String, JoinHandle<HandlerResult>>>>;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "I track your transit in your area and send you notifications with current location.\nThese commands are supported:"
)]
enum Command {
    #[command(description = "Display help menu showing the commands list")]
    Help,
    #[command(description = "Start tracking your transit.")]
    Start,
    #[command(description = "Cancel the tracking.")]
    Cancel,
}

#[derive(Clone, Default)]
enum State {
    #[default]
    Start,
    ReceiveCity,
    ReceiveAddress {
        city: String,
    },
    ReceiveStop {
        city: String,
        addr: String,
        stations: Vec<Station>,
    },
    ReceiveTransit {
        city: String,
        addr: String,
        stations: Vec<Station>,
        stop: String,
        stop_id: String,
    },
    ReceiveMinutes {
        city: String,
        addr: String,
        stations: Vec<Station>,
        stop: String,
        stop_id: String,
        transit: String,
    },
    ReceiveCancel,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();
    log::info!("Starting 'Where is my transit' BOT ...");
    
    // Load env variables
    dotenv().ok();
    let bot = Bot::from_env();

    let command_handler = filter_command::<Command, _>()
        .branch(
            case![State::Start]
                .branch(case![Command::Help].endpoint(help))
                .branch(case![Command::Start].endpoint(start)),
        )
        .branch(case![Command::Cancel].endpoint(cancel));

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(case![State::ReceiveCity].endpoint(receive_city))
        .branch(case![State::ReceiveAddress { city }].endpoint(receive_address))
        .branch(endpoint(invalid_state));

    let callback_query_handler = Update::filter_callback_query()
        .branch(
            case![State::ReceiveStop {
                city,
                addr,
                stations
            }]
            .endpoint(receive_stop),
        )
        .branch(
            case![State::ReceiveTransit {
                city,
                addr,
                stations,
                stop,
                stop_id,
            }]
            .endpoint(receive_transit),
        )
        .branch(
            case![State::ReceiveMinutes {
                city,
                addr,
                stations,
                stop,
                stop_id,
                transit,
            }]
            .endpoint(receive_minutes),
        )
        .branch(case![State::ReceiveCancel].endpoint(receive_cancel));

    let dial = dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler);

    // Shared HashMap of JoinHandles of tasks to be able to cancel the timer.
    let tasks: Arc<Mutex<HashMap<String, JoinHandle<HandlerResult>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    Dispatcher::builder(bot, dial)
        .dependencies(deps![InMemStorage::<State>::new(), tasks])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

//////////////////////////////////////////////////////////
// State handlers
//////////////////////////////////////////////////////////
async fn invalid_state(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(
        msg.chat.id,
        "Unable to handle the message. Type /help to see the usage.",
    )
    .await?;
    Ok(())
}

async fn cancel(bot: Bot, dialogue: MyDialogue, msg: Message, tasks: MyTasksMap) -> HandlerResult {
    let user = get_user_id(&msg);
    {
        let mut t = tasks.lock().unwrap();
        if let Some(v) = t.get(&user) {
            v.abort();
            t.remove(&user);
        }
    }
    bot.send_message(
        msg.chat.id,
        "🚫 Cancelled! You can start over using /start.",
    )
    .await?;
    dialogue.exit().await?;
    Ok(())
}

async fn start(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    let user_id = match &msg.kind {
        MessageKind::Common(MessageCommon { from, .. }) => Some(from.as_ref().unwrap().id),
        _ => None,
    };

    let UserId(id_num) = user_id.unwrap();
    match get_user_data(id_num.to_string()) {
        Ok(data) => {
            let stations = get_nearby_stations(data.lat.clone(), data.lon.clone()).await?;
            let mut stations_names = stations
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<&str>>();

            let mut kb_buttons: Vec<&str> = vec![];
            kb_buttons.append(&mut stations_names);
            kb_buttons.push("<< Change address");

            let kb = make_inline_keyboard(kb_buttons, 2);
            // Parse station names as keyboard buttons
            // let kb = make_inline_keyboard(stations_names.clone(), 2);
            bot.send_message(
                msg.chat.id,
                format!(
                    "I found your last used address: \n<b>{}, {} 📍</b>\n
Now please select which transit station you want to track 👀.\n
Here are the nearby transit stations:",
                    data.addr, data.city
                ),
            )
            .parse_mode(Html)
            .reply_markup(kb)
            .await?;
            dialogue
                .update(State::ReceiveStop {
                    city: data.city,
                    addr: data.addr,
                    stations,
                })
                .await?
        }
        Err(_) => {
            bot.send_message(
                msg.chat.id,
                format!("🔰 Let's start tracking a transit 🚌🚇!\n\nWhat city do you live in?"),
            )
            .reply_markup(KeyboardRemove::new())
            .await?;
            // println!("{:#?}", msg);
            dialogue.update(State::ReceiveCity).await?;
        }
    }
    Ok(())
}

async fn receive_city(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    match msg.text().map(ToOwned::to_owned) {
        Some(city) => {
            bot.send_message(
                msg.chat.id,
                format!("Awesome so you live in <b>{}</b> 🏙!
\n\nWhat is your locations <b>street address</b> so i can search for nearby transit stops?
You can also send me a <b>location</b> 📍 instead!", city),
            ).parse_mode(Html)
            .await?;
            dialogue.update(State::ReceiveAddress { city }).await?;
        }
        None => {
            bot.send_message(
                msg.chat.id,
                "❌ Please, send me your city that you live in, so i can start tracking.",
            )
            .await?;
        }
    }
    Ok(())
}

async fn receive_address(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    city: String,
) -> HandlerResult {
    let location = msg.location();
    let address = msg.text().map(ToOwned::to_owned);
    if location.is_none() && address.is_none() {
        bot.send_message(
            msg.chat.id,
            "❌ Please, send me your address, so i can start tracking.",
        )
        .await?;
        return Ok(());
    }
    let addr;

    // Store info of user for next time
    let user_id = match &msg.kind {
        MessageKind::Common(MessageCommon { from, .. }) => Some(from.as_ref().unwrap().id),
        _ => None,
    };

    let UserId(id_num) = user_id.unwrap();

    // Get geocode if location was sent instead of an address
    let geocode = match location {
        Some(loc) => {
            addr = fetch_address(loc.latitude.to_string(), loc.longitude.to_string()).await?;
            (loc.latitude.to_string(), loc.longitude.to_string())
        },
        None => {
            addr = address.unwrap();
            fetch_geocode(addr.clone(), city.clone()).await?
        }
    };

    let stations = get_nearby_stations(geocode.0.clone(), geocode.1.clone()).await?;
    let mut stations_names = stations
        .iter()
        .map(|x| x.name.as_str())
        .collect::<Vec<&str>>();

    let user_data = UserData {
        id: id_num.to_string(),
        city: city.clone(),
        addr: addr.clone(),
        lat: geocode.0,
        lon: geocode.1,
    };
    store_user_data(user_data).unwrap();

    let mut kb_buttons: Vec<&str> = vec![];
    kb_buttons.append(&mut stations_names);
    kb_buttons.push("<< Change address");

    let kb = make_inline_keyboard(kb_buttons, 2);
    bot.send_message(
        msg.chat.id,
        format!(
            "Thank you! So your address is: \n<b>{}, {} 📍</b>\n\n
Now please select which transit station you want to track 👀.\n
Here are the nearby transit stations:",
            addr, city
        ),
    )
    .parse_mode(Html)
    .reply_markup(kb)
    .await?;

    dialogue
        .update(State::ReceiveStop {
            city,
            addr,
            stations,
        })
        .await?;
    Ok(())
}

async fn receive_stop(
    bot: Bot,
    dialogue: MyDialogue,
    (city, addr, stations): (String, String, Vec<Station>),
    q: CallbackQuery,
) -> HandlerResult {
    if let Some(stop) = &q.data {
        let chat_id = q.message.as_ref().unwrap().chat.id;
        let message_id = q.message.as_ref().unwrap().id;
        let null_kb = InlineKeyboardMarkup::default();

        if stop.starts_with("<<") {
            bot.edit_message_text(chat_id, message_id, "Ok, send me the new <b>street address</b> or a <b>location</b> 📍.")
                .parse_mode(Html)
                .reply_markup(null_kb.clone())
                .await?;
            dialogue
                .update(State::ReceiveAddress { city: city.clone() })
                .await?;
            
            return Ok(());
        };

        // Remove buttons from last send msg
        bot.edit_message_reply_markup(chat_id, message_id)
            .reply_markup(null_kb)
            .await?;

        let stop_id = stations
            .iter()
            .find(|&x| x.name.starts_with(stop))
            .unwrap()
            .id
            .to_owned();

        let departures = get_departures(stop_id.clone()).await?;

        if departures.len() == 0 {
            bot.send_message(
                dialogue.chat_id(),
                format!("😟 Unfortunately, no transit departures were found from this station at this time. Please /start over!")
            ).await?;

            dialogue.exit().await?;
            return Ok(());
        }

        let mut dep_names: Vec<String> = vec![];

        // Format departure info
        let mut departure_info = String::new();
        for dep in departures.iter() {
            let time = chrono::DateTime::parse_from_rfc3339(&dep.planned)
                .unwrap()
                .time()
                .format("%H:%M");

            // Add delay in minutes between parenthesis in the infos (+delay)
            let delay = match dep.delay {
                None => format!(""),
                Some(del) => {
                    if del == 0 {
                        format!("")
                    } else {
                        format!(" (+{})", del / 60)
                    }
                }
            };

            departure_info = format!(
                "{}\n--------------------\n<b>{}</b>, to <b>{}</b> on <b>{}</b>{}",
                departure_info, dep.name, dep.direction, time, delay
            );

            // Add direction to departure names for buttons
            let dep_name = format!("{} ({})", dep.name, dep.direction);
            dep_names.push(dep_name);
        }
        let kb = make_inline_keyboard(dep_names.iter().map(|x| x.as_ref()).collect(), 2);

        // Output departure info
        bot.send_message(
            dialogue.chat_id(),
            format!(
                "🚏 Infos for selected station (+ mins delay): <b>{}</b>\n{}\n--------------------",
                stop, departure_info
            ),
        )
        .parse_mode(Html)
        .await?;

        // Send buttons for the transit departures
        bot.send_message(dialogue.chat_id(), "Select a transit:")
            .reply_markup(kb)
            .await?;

        dialogue
            .update(State::ReceiveTransit {
                city,
                addr,
                stations,
                stop: stop.to_string(),
                stop_id,
            })
            .await?;
    }
    Ok(())
}

async fn receive_transit(
    bot: Bot,
    dialogue: MyDialogue,
    (city, addr, stations, stop, stop_id): (String, String, Vec<Station>, String, String),
    q: CallbackQuery,
) -> HandlerResult {
    if let Some(transit) = &q.data {
        let kb = make_inline_keyboard(vec!["1", "2", "3"], 3);

        // Delete last update message
        bot.delete_message(dialogue.chat_id(), q.message.unwrap().id)
            .await?;

        // Send buttons for the timer update rate
        bot.send_message(
            dialogue.chat_id(),
            "Select how many <b>minutes</b> between each update:",
        )
        .parse_mode(Html)
        .reply_markup(kb)
        .await?;

        dialogue
            .update(State::ReceiveMinutes {
                city,
                addr,
                stations,
                stop,
                stop_id,
                transit: transit.to_owned(),
            })
            .await?;
    }
    Ok(())
}

async fn receive_minutes(
    bot: Bot,
    dialogue: MyDialogue,
    (_city, _addr, _stations, stop, stop_id, transit): (
        String,
        String,
        Vec<Station>,
        String,
        String,
        String,
    ),
    q: CallbackQuery,
    tasks: MyTasksMap,
) -> HandlerResult {
    if let Some(update_time) = &q.data {
        let dial = dialogue.clone();
        // Time between each tracking update
        let mut update_time = update_time.parse::<i64>().unwrap();
        let msg = q.message.clone().unwrap();
        let mut msg_clone = msg.clone();
        let mut curr_loc_msg: Option<Message> = None;
        let mut loc_msg: Option<Message> = None;
        let transit_clone = transit.clone();

        bot.send_message(
            dialogue.chat_id(),
            format!(
                "The timer is going to update every <b>{}</b> minute(s)!",
                update_time
            ),
        )
        .parse_mode(Html)
        .await?;

        let mut time_now = msg_clone.date.clone() - Duration::minutes(update_time);
        let mut interval_timer =
            tokio::time::interval(Duration::minutes(update_time).to_std().unwrap());

        // Spawn task for repeated updates
        let task: JoinHandle<HandlerResult> = tokio::spawn(async move {
            loop {
                interval_timer.tick().await;
                // Add the difference to the time that passed since timer's tick
                time_now += Duration::minutes(update_time);

                // Fetch the list of departuring transits from the stop (station)
                let deps = get_departures(stop_id.clone()).await?;

                // Get the departure of selected transit considering direction the user has selected
                let dep = match deps.iter().find(|&x| {
                    // Extract name and direction from button to compare transit departures
                    let first_parent = transit_clone.find("(").unwrap();
                    let last_parent = transit_clone.len();
                    let direction = transit_clone[first_parent + 1..last_parent - 1].to_string();
                    let transit_name = transit_clone[0..first_parent - 1].to_string();

                    &x.name == &transit_name && &x.direction == &direction
                }) {
                    Some(d) => d,
                    None => break,
                };

                // Parse planned departure time
                let mut dep_time = DateTime::parse_from_rfc3339(&dep.planned).unwrap();

                // Add the delay if exists
                if let Some(del) = dep.delay {
                    dep_time += Duration::seconds(del);
                }
                // Calculate duration till departure
                let dur = dep_time.signed_duration_since(time_now);

                // When below update time, update interval timer to the duration time
                if update_time != 1 && dur.num_minutes() > 0 && dur.num_minutes() < update_time {
                    update_time = dur.num_minutes();
                    interval_timer =
                        tokio::time::interval(Duration::minutes(update_time).to_std().unwrap());
                    // Tick the first time which is instant
                    interval_timer.tick().await;
                }

                // Stop if transit already departured
                if time_now > dep_time {
                    break;
                }

                let kb = make_inline_keyboard(vec!["<< Cancel"], 1);

                // Delete last message including the msg of the update coming from the previous iteration
                bot.delete_message(dial.chat_id(), msg_clone.id).await?;

                // Delete location messages if it exists
                if let Some(m) = &curr_loc_msg {
                    bot.delete_message(dial.chat_id(), m.id).await?;
                }
                if let Some(m) = &loc_msg {
                    bot.delete_message(dial.chat_id(), m.id).await?;
                }

                // Send location of transit if current position information is provided
                if let Some(pos) = &dep.curr_position {
                    // unwrap() since Location.lat is a string that contains always a number
                    curr_loc_msg = Some(
                        bot.send_message(dial.chat_id(), "Current position of the transit:")
                            .await?,
                    );
                    loc_msg = Some(
                        bot.send_location(
                            dial.chat_id(),
                            pos.lat.parse::<f64>().unwrap(),
                            pos.lon.parse::<f64>().unwrap(),
                        )
                        .await?,
                    );
                }

                if dur.num_minutes() == 0 {
                    // Update timer when duration below 1 minute
                    update_time = 1;
                    interval_timer =
                        tokio::time::interval(Duration::minutes(update_time).to_std().unwrap());
                    interval_timer.tick().await;

                    msg_clone = bot
                        .send_message(
                            dial.chat_id(),
                            format!(
                                "🔔 Your transit: <b>{}</b> 🚌 should arrive now!",
                                &transit_clone
                            ),
                        )
                        .parse_mode(Html)
                        .reply_markup(kb.clone())
                        .await?;
                } else {
                    msg_clone = bot
                        .send_message(
                            dial.chat_id(),
                            format!(
                                "🔔 Your transit: <b>{}</b> 🚌 arrives in <b>{}</b> minutes ⌛!",
                                &transit_clone,
                                dur.num_minutes()
                            ),
                        )
                        .parse_mode(Html)
                        .reply_markup(kb)
                        .await?;
                }
            }

            // Delete last update message
            bot.delete_message(dial.chat_id(), msg_clone.id).await?;

            bot.send_message(
                dial.chat_id(),
                format!(
                    "🔔 Your transit: <b>{}</b> 🚌 is departuring from <b>{}</b>!",
                    transit_clone, stop
                ),
            )
            .parse_mode(Html)
            .await?;

            dial.exit().await?;
            return Ok(());
        });

        // Insert Task-handle in the HashMap with the associated user
        // Msg could be put in the hashmap, to edit from the cancel fn
        {
            let mut t = tasks.lock().unwrap();
            let user = q.from.id.to_string();
            t.insert(user, task);
        }

        dialogue.update(State::ReceiveCancel).await?;
    }
    Ok(())
}

async fn receive_cancel(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    tasks: MyTasksMap,
) -> HandlerResult {
    let user = q.from.id.to_string();
    {
        let mut t = tasks.lock().unwrap();
        if let Some(v) = t.get(&user) {
            v.abort();
            t.remove(&user);
        }
    }
    let null_kb = InlineKeyboardMarkup::default();
    bot.edit_message_reply_markup(dialogue.chat_id(), q.message.as_ref().unwrap().id)
        .reply_markup(null_kb)
        .await?;

    bot.send_message(
        dialogue.chat_id(),
        "🚫 Cancelled! You can start over using /start!",
    )
    .await?;
    dialogue.exit().await?;
    Ok(())
}

fn get_user_id(msg: &Message) -> String {
    let user_id = match &msg.kind {
        MessageKind::Common(MessageCommon { from, .. }) => Some(from.as_ref().unwrap().id),
        _ => None,
    };

    let UserId(id_num) = user_id.unwrap();
    id_num.to_string()
}

//////////////////////////////////////////////////////////
// Keyboards
//////////////////////////////////////////////////////////
/// Creates a keyboard made by buttons in a big column.
fn make_inline_keyboard(list: Vec<&str>, chunks: usize) -> InlineKeyboardMarkup {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];

    for values in list.chunks(chunks) {
        let row = values
            .iter()
            .map(|&value| InlineKeyboardButton::callback(value.to_owned(), value.to_owned()))
            .collect();

        keyboard.push(row);
    }

    InlineKeyboardMarkup::new(keyboard)
}

/// Creates a keyboard made by buttons in a big column.
fn _make_keyboard(list: Vec<&str>, chunks: usize) -> KeyboardMarkup {
    let mut keyboard: Vec<Vec<KeyboardButton>> = vec![];

    for values in list.chunks(chunks) {
        let row = values
            .iter()
            .map(|&value| KeyboardButton::new(format!("{}", value)))
            .collect();

        keyboard.push(row);
    }
    KeyboardMarkup::new(keyboard)
}

/// Creates a keyboard made by buttons in a big column.
fn _make_reply_keyboard(list: Vec<&str>, chunks: usize) -> ReplyMarkup {
    let mut keyboard: Vec<Vec<KeyboardButton>> = vec![];

    for values in list.chunks(chunks) {
        let row = values
            .iter()
            .map(|&value| KeyboardButton::new(format!("{}", value)))
            .collect();

        keyboard.push(row);
    }
    ReplyMarkup::keyboard(keyboard)
}
