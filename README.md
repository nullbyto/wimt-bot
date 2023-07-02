# Where Is My Transit Bot (wimt-bot)

This is a telegram chat bot that tracks nearby transit stations (bus or train), where you select a transit that you want to track and get information about, like the arrival, 
departure and location of the transit in the map, with a timer function to notify you every x minutes.

### Works only in Germany!! Since it uses a wrapper API around the Deutsche Bahn API https://v5.db.transport.rest

<p align="center">
    <img src="static/showcase.gif" width="225" height="" />
</p>

## Features
- Display nearby transit stations
- Shows transit arrival and departure times + delays
- Track selected transit (for example bus) when it arrives with
- Timer to send updated information about the transit every x minutes
- Location of the transit on the map

## Setup
You will need to setup a `.env` file with these variables:
```
TELOXIDE_TOKEN="<TOKEN>"
LOCATIONIQ_TOKEN="<TOKEN>"
USER_DATA_PATH="./userdata.json"
```
where:
- `TELOXIDE_TOKEN` is the telegram API token that you receive when creating a bot on telegram.
- `LOCATIONIQ_TOKEN` is the token for the API for the service from [LocationIQ](https://locationiq.com) to fetch geocode data of the address provided by the users.
- `USER_DATA_PATH` is the path to where you want to store the users data in JSON format.
