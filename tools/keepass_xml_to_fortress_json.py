#!/usr/bin/env python
# Converts a KeePass XML file (which can be exported from KeePass) to a Fortress JSON
# The resulting Fortress JSON can be piped through GZip and then encrypted using "fortress --encrypt"
# WARNING: This conversion is LOSSY.
# WARNING: Currently does not translate over attached files, history, and a bunch of other things.
# WARNING: Flattens all groups, because Fortress doesn't support groups (yet?)
# WARNING: Incomplete and UNTESTED
import xml.etree.ElementTree
import sys
import os
import time
import json


database = {'entries': []}


def handle_group (parent):
	if parent.find('Name') is not None:
		if parent.find('Name').text == 'Recycle Bin':
			# Skip Recycle Bin
			return
	
	for group in parent.findall("Group"):
		handle_group (group)
	
	for entry in parent.findall("Entry"):
		handle_entry (entry)


def handle_entry (entry):
	entry_data = {'Title': '', 'Notes': '', 'Password': '', 'URL': '', 'UserName': ''}

	for e in entry.findall("String"):
		key = e.find('Key').text
		value = e.find('Value').text

		if key in entry_data and value is not None:
			entry_data[key] = value
	
	entry = {}
	entry['id'] = os.urandom (32).encode ('hex')
	entry['history'] = [
		{
			'title': entry_data['Title'],
			'username': entry_data['UserName'],
			'password': entry_data['Password'],
			'url': entry_data['URL'],
			'notes': entry_data['Notes'],
			'time_created': int(time.time())
		}
	]
	
	database['entries'].append (entry)


e = xml.etree.ElementTree.parse(sys.argv[1])
e = e.find('Root')

handle_group (e)

print json.dumps (database)