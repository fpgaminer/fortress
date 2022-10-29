#!/usr/bin/env python3
# Converts a decrypted and decompressed Fortress v1 JSON file to a Fortress v2 JSON file
# The resulting Fortress v2 JSON can be encrypted using "fortress encrypt"
import json
import sys


NS_COUNTER = 0


def main():
	if len(sys.argv) != 3:
		print("Usage: %s <Fortress v1 JSON file> <username>" % sys.argv[0])
		sys.exit(1)

	# Read input as JSON
	with open(sys.argv[1], 'r') as f:
		database = json.load(f)
	
	# Convert to Fortress v2 JSON
	database = convert_database(database, sys.argv[2])

	# Write output as JSON
	print(json.dumps(database))

def convert_history_item(history):
	global NS_COUNTER

	# Every history item must have a 'time_created'
	assert(len(history) >= 1)

	# time_created must be an integer
	assert(isinstance(history['time_created'], int))
	time_created = history['time_created']
	del history['time_created']

	# Verify that all other keys are expected
	assert(all(key in ['title', 'username', 'password', 'url', 'notes'] for key in history.keys()))

	# And all are strings
	assert(all(isinstance(value, str) for value in history.values()))

	# Fortress v2 uses nanoseconds instead of seconds
	# We also use a global counter to ensure that all timestamps are unique,
	# since the v1 format does not guarantee this
	time = time_created * 1000000000 + NS_COUNTER
	NS_COUNTER += 1
	assert(NS_COUNTER < 1000000000)

	return {
		# Fortress v2 uses nanoseconds instead of seconds
		'time': time,
		'data': history,
	}


def convert_entry(entry):
	assert(len(entry) == 2)
	assert('history' in entry)
	assert('id' in entry)

	# ID must be 32 bytes, hex-encoded
	assert(len(entry['id']) == 64)
	assert(all(c in '0123456789abcdef' for c in entry['id']))

	history = [convert_history_item(item) for item in entry['history']]

	# Sort history by time ascending
	history.sort(key=lambda item: item['time'])

	time_created = history[0]['time']

	return {
		'type': 'Entry',
		'id': entry['id'],
		'history': history,
		'time_created': time_created,
	}

def convert_database(database, username):
	# 'entries' should be the only key
	assert('entries' in database)
	assert(len(database) == 1)

	objects = [convert_entry(entry) for entry in database['entries']]

	# Create root directory
	# NOTE: We have to fabricate various timestamps
	root_history = [{'action': {'Add': object['id']}, 'time': object['time_created']} for object in objects]
	root_history.sort(key=lambda item: item['time'])
	root_history.insert(0, {'action': {'Rename': 'My Passwords'}, 'time': root_history[0]['time'] - 1})

	root = {
		'type': 'Directory',
		'id': '0' * 64,
		'history': root_history,
		'time_created': root_history[0]['time'],
	}

	objects.append(root)

	# All other fields in the database can be null; Fortress will fill them in as needed
	return {
		'objects': objects,
		'sync_parameters': {'username': username},
	}


if __name__ == '__main__':
	main()