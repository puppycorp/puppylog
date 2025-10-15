diesel::table! {
	devices (id) {
		id -> Text,
		send_logs -> Bool,
		filter_level -> Integer,
		logs_size -> BigInt,
		logs_count -> BigInt,
		created_at -> Timestamp,
		last_upload_at -> Nullable<Timestamp>,
		send_interval -> Integer,
	}
}

diesel::table! {
	device_props (device_id, key, value) {
		device_id -> Text,
		key -> Text,
		value -> Text,
	}
}

diesel::table! {
	log_segments (id) {
		id -> Integer,
		bucket_id -> Nullable<Integer>,
		device_id -> Nullable<Text>,
		first_timestamp -> Timestamp,
		last_timestamp -> Timestamp,
		original_size -> BigInt,
		compressed_size -> Nullable<BigInt>,
		logs_count -> BigInt,
		created_at -> Timestamp,
	}
}

diesel::table! {
	segment_props (segment_id, key, value) {
		segment_id -> Integer,
		key -> Text,
		value -> Text,
	}
}

diesel::table! {
	migrations (id) {
		id -> Integer,
		name -> Text,
		applied_at -> Timestamp,
	}
}

diesel::allow_tables_to_appear_in_same_query!(
	devices,
	device_props,
	log_segments,
	segment_props,
	migrations,
);
