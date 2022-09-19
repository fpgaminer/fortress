// A simplified in-memory Fortress server used for sync tests
use data_encoding::HEXLOWER_PERMISSIVE;
use fortresscrypto::MacTag;
use libfortress::ID;
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashMap, thread};
use tiny_http::{Method, Response, Server};


// Starts a server and returns the address it is listening on
pub fn server() -> String {
	let mut db = HashMap::new();
	let server = Server::http("127.0.0.1:0").unwrap();
	let addr = server.server_addr().to_string();

	let api = |url: &str, json: JsonRequest, db: &mut HashMap<ID, (Vec<u8>, MacTag)>| match url {
		"/update_object" => {
			db.insert(
				json.object_id.unwrap(),
				(HEXLOWER_PERMISSIVE.decode(json.data.unwrap().as_bytes()).unwrap(), json.data_mac.unwrap()),
			);

			json!({ "error": null })
		},
		"/diff_objects" => {
			let mut unknown_ids = Vec::new();
			let mut updates = Vec::new();

			for object in json.objects.unwrap() {
				match db.get(&object.id) {
					Some(db_object) => {
						if db_object.1 != object.mac {
							updates.push(json!({
								"id": object.id,
								"data": HEXLOWER_PERMISSIVE.encode(&db_object.0),
								"mac": db_object.1,
							}));
						}
					},
					None => unknown_ids.push(object.id.clone()),
				}
			}

			json!({
				"error": null,
				"updates": updates,
				"unknown_ids": unknown_ids,
			})
		},
		"/get_object" => match db.get(&json.object_id.unwrap()) {
			Some(object) => {
				json!({
					"error": null,
					"data": HEXLOWER_PERMISSIVE.encode(&object.0),
					"mac": object.1,
				})
			},
			None => {
				json!({
					"error": "Unknown Object",
				})
			},
		},
		_ => panic!("404"),
	};

	thread::spawn(move || {
		for mut request in server.incoming_requests() {
			assert_eq!(request.method(), &Method::Post);
			let json: JsonRequest = serde_json::from_reader(request.as_reader()).unwrap();
			let response = api(request.url(), json, &mut db);

			request.respond(Response::from_string(response.to_string())).unwrap()
		}
	});

	"http://".to_string() + &addr
}


// Handles all possible requests
#[derive(Deserialize)]
struct JsonRequest {
	#[serde(rename = "user_id")]
	_user_id: ID, // Must be provided for API calls, but is not checked for tests.
	#[serde(rename = "user_key")]
	_user_key: ID, // Must be provided for API calls, but is not checked for tests.
	object_id: Option<ID>,
	data: Option<String>,
	data_mac: Option<MacTag>,
	objects: Option<Vec<JsonRequestObject>>,
}

#[derive(Deserialize)]
struct JsonRequestObject {
	id: ID,
	mac: MacTag,
}
