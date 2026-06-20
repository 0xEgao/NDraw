mod common;

use std::error::Error;

use ndraw_proto::{decode_client, decode_server, encode_client, encode_server};

#[test]
fn every_client_message_round_trips() -> Result<(), Box<dyn Error>> {
    for expected in common::client_messages() {
        let frame = encode_client(&expected)?;
        let actual = decode_client(&frame)?;
        assert_eq!(actual, expected);
    }
    Ok(())
}

#[test]
fn every_server_message_round_trips() -> Result<(), Box<dyn Error>> {
    for expected in common::server_messages()? {
        let frame = encode_server(&expected)?;
        let actual = decode_server(&frame)?;
        assert_eq!(actual, expected);
    }
    Ok(())
}
