# ebus_mqtt

## EBUS interface (ebusd.eu) enhanced protocol -> MQTT (HomeAssistant) data transcoder written in Rust. 

Application reads **config.json** file for connection and parsing parameters, connects to EBUS interface (I'm using v5 version), parses incoming EBUS data and emits MQTT messages according to defined appliance file (eg. **ariston.json**)

## Why? 
Because I don't understand why **ebusd** is using CSV for parsing definitions.

## ToDo::
Quite a lot... really.

### Any help highly appreciated !
I'm doing this as a hobby, in my free time. Don't expect much.




---
Disclaimer:
    - this is my first project in Rust, you've been warned.
