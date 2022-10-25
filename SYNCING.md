# Fortress Sync Details

This document describes the sync protocol used by Fortress.

## Overview

Fortress can sync its database with a backend "Fortress Server".  This allows a user to have the same database on multiple machines and keep them in sync.  It's also useful as part of a user's backup strategy.  Using only their username and password, a user can completely restore their password database from any Fortress Server they've synced with.

Generally speaking, syncing is implemented in a straightforward way.  The client asks the server for a list of Objects it knows (IDs and SIVs).  From this the client can determine any Objects it's missing, Objects that may need to be downloaded and merge, and Objects it might need to upload.  Because all Objects in Fortress are append only and keep a timestamped history of changes, it's easy for Fortress to merge changes non-destructively.

The exact API is described in more detail in the Fortress Server project itself.

All Objects are encrypted (see [ENCRYPTION.md](ENCRYPTION.md)) and authenticated, making this whole process end-to-end encrypted.  The server doesn't have access to the user's password and, in some instances, might not even have access to the user's username, instead only authenticating users based on a hash of their username and a cryptographically derived login token.