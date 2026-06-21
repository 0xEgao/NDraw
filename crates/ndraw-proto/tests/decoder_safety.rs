use std::panic::{AssertUnwindSafe, catch_unwind};

use ndraw_proto::{
    ClientMessage, DrawOp, Point, StrokeId, decode_client, decode_server, encode_client,
    limit::{CANVAS_HEIGHT, CANVAS_WIDTH, MAX_FRAME_BYTES, MAX_POINTS_PER_BATCH},
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_frames_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..(MAX_FRAME_BYTES + 32))) {
        let client = catch_unwind(AssertUnwindSafe(|| decode_client(&bytes)));
        let server = catch_unwind(AssertUnwindSafe(|| decode_server(&bytes)));

        prop_assert!(client.is_ok());
        prop_assert!(server.is_ok());
    }

    #[test]
    fn valid_point_batches_round_trip(
        stroke_id in any::<u32>(),
        sequence in any::<u16>(),
        coordinates in prop::collection::vec(
            (0..=CANVAS_WIDTH, 0..=CANVAS_HEIGHT),
            1..=MAX_POINTS_PER_BATCH,
        ),
    ) {
        let points = coordinates
            .into_iter()
            .map(|(x, y)| Point { x, y })
            .collect();
        let expected = ClientMessage::Draw(DrawOp::Points {
            stroke_id: StrokeId(stroke_id),
            sequence,
            points,
        });

        let encoded = encode_client(&expected);
        prop_assert!(encoded.is_ok());
        if let Ok(frame) = encoded {
            match decode_client(&frame) {
                Ok(actual) => prop_assert_eq!(actual, expected),
                Err(error) => prop_assert!(false, "valid frame failed to decode: {error}"),
            }
        }
    }
}
