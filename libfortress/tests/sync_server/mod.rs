// A simplified in-memory Fortress server used for sync tests
use data_encoding::HEXLOWER_PERMISSIVE;
use fortresscrypto::SIV;
use libfortress::ID;
use std::{collections::HashMap, thread};
use tiny_http::{Method, Response, Server};


// Starts a server and returns the address it is listening on
pub fn server() -> String {
	let mut db = HashMap::new();
	let server = Server::http("127.0.0.1:0").unwrap();
	let addr = server.server_addr().to_string();

	let api = |method: Method, url: Vec<&str>, body: Vec<u8>, db: &mut HashMap<ID, Vec<u8>>| match (method, url.as_slice()) {
		(Method::Get, ["objects"]) => {
			let response: Vec<_> = db
				.iter()
				.map(|(id, data)| (id, HEXLOWER_PERMISSIVE.encode(data[data.len() - 32..].as_ref())))
				.collect();
			Response::from_string(serde_json::to_string(&response).unwrap())
		},
		(Method::Get, ["object", id]) => {
			let id = ID::from_slice(&HEXLOWER_PERMISSIVE.decode(id.as_bytes()).unwrap()).unwrap();
			match db.get(&id) {
				Some(data) => Response::from_data(data.clone()),
				None => Response::from_string("".to_string()).with_status_code(404),
			}
		},
		(Method::Post, ["object", id, old_siv]) => {
			let id = ID::from_slice(&HEXLOWER_PERMISSIVE.decode(id.as_bytes()).unwrap()).unwrap();
			let old_siv = SIV::from_slice(&HEXLOWER_PERMISSIVE.decode(old_siv.as_bytes()).unwrap()).unwrap();
			if let Some(data) = db.get(&id) {
				if old_siv != SIV::from_slice(&data[data.len() - 32..]).unwrap() {
					return Response::from_string("".to_string()).with_status_code(409);
				}
			}
			db.insert(id, body);
			Response::from_string("".to_string())
		},
		_ => panic!("404"),
	};

	thread::spawn(move || {
		for mut request in server.incoming_requests() {
			let method = request.method().clone();
			let url = request.url().to_owned();
			let url = url.split('/').skip(1).collect::<Vec<_>>();
			let mut body = Vec::new();
			request.as_reader().read_to_end(&mut body).unwrap();

			// Make sure auth header is present and the right format
			let auth = request.headers().iter().find(|h| h.field.equiv("Authorization")).unwrap().value.to_string();
			let auth = auth.split(' ').skip(1).next().unwrap();
			let auth = HEXLOWER_PERMISSIVE.decode(auth.as_bytes()).unwrap();
			if auth.len() != 64 {
				request.respond(Response::from_string("".to_string()).with_status_code(401)).unwrap();
				continue;
			}

			request.respond(api(method, url, body, &mut db)).unwrap();
		}
	});

	"http://".to_string() + &addr
}
