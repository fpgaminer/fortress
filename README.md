# Fortress
Remember one password, securely manage the rest.  Fortress is insanely secure and automatically synced between devices.

## Status
Just started building this project.

## Project goals
I decided to write my own password manager.  I wanted a password manager that I could trust, with automatic sync, and an easy to use UI.  KeePass, my usual choice, doesn't offer automatic sync and its UI is a little rough around the edges.  Things like LastPass have a notortious history.  So I'm building my own.

* Automatic sync - Easy to use across devices and easy backups.
* Strong security
* Written in Rust - Less bugs, cleaner code.
* Simple design - I opt for simplicity over performance; easier to audit the code and design.
* Trustless - The code is open source.  Data sync is end-to-end encrypted.

## Database Format

### Motivation

At its core, the on-disk format for Fortress is just encrypted JSON, because JSON is simple, portable, and human readable.

A Fortress database consists of a collection of Objects, each of which is either a Directory or an Entry.  A Directory is just a list of other objects.  This builds a directory tree.  An Entry is basically just a HashMap; so it's just a key-value store, making it easy to adapt the database to new features in the future.

Every object in a Fortress database stores a history so users can roll back to previous passwords and undo mistakes.

Using standard formats like JSON means that Fortress databases can be manipulated using existing tooling; even on the Linux command line.  Though this won't be common it's useful to have if, for example, someone wants to write third-party tools that work with Fortress databases.

The only caveat is encryption.  There's no good, standard encryption format.  So Fortress has to use its own, but again it's very simple.  On the command line Fortress can be used to encrypt/decrypt payloads using its encryption format, so it's still possible to easily get at the JSON inside a database.

### Format (V1)

    header_string:  UTF-8 NULL terminated string
    scrypt_log_n:   scrypt parameter (u8)
    scrypt_r:       scrypt parameter (u32 little endian)
    scrypt_p:       scrypt parameter (u32 little endian)
    scrypt_salt:    scrypt parameter (u8 * 32)
    pbkdf2_salt:    pbkdf2 parameter (u8 * 32)
    payload:        The encrypted data (*)
    mac_tag:        HMAC of all proceeding data (u8 * 32)
    checksum:       SHA-256 of all proceeding data (u8 * 32)


`header_string` (e.g. fortress1-scrypt-chacha20) specifies the format version (e.g. 1) and the encryption used.  V1 only supports one encryption standard, scrypt-chacha20.  In future versions different encryption standards might be supported, and in those cases the file format may be different, e.g. if Poly1305 is used the "mac_tag" field won't be 32 bytes.

To decrypt scrypt-chacha20:

    verify_sha256_checksum (header+payload+mac_tag, checksum);

    master_key = scrypt (scrypt_log_n, scrypt_r, scrypt_p, scrypt_salt, password);

    chacha_key: [u8; 32], chacha_nonce: [u8; 8], hmac_key: [u8; 32] = pbkdf2_hmac_sha256 (1, pbkdf2_salt, master_key);

    // Do NOT save pbkdf2_salt; throw it away now.

    verify_hmac_sha256 (hmac_key, header+payload, mac_tag);

    plaintext = chacha20_decrypt (chacha_key, chacha_nonce, payload);


We use scrypt to derive a `master_key` and then use PBKDF2 to expand the `master_key` into all the keys we need for the rest of the cryptographic primitives; in this case ChaCha20 and HMAC.  (See the description of encryption for why PBKDF2 is used after scrypt).

The `checksum` field lets us determine if the database is corrupt.  HMAC kinda does that, but can't differentiate a bad password versus corruption.

Of course, `mac_tag` must **always** be checked before proceeding to chacha20_decrypt.

`plaintext` is now just GZip'd JSON.  The JSON data structure can be understood by looking at the relavant structs in the library code (Database, Entry, EntryData, etc), but because it's JSON it's fairly self explainitory on its own.


To encrypt scrypt-chacha20:

    pbkdf2_salt: [u8; 32] = random (32);
	chacha_key: [u8; 32], chacha_nonce: [u8; 8], hmac_key: [u8; 32] = pbkdf2_hmac_sha256 (1, pbkdf2_salt, master_key);

	payload = chacha20_encrypt (chacha_key, chacha_nonce, plaintext);

	mac_tag = hmac_sha256 (hmac_key, header+payload);
	checksum = sha256 (header+payload+mac_tag);


Generate a different `pbkdf2_salt` **everytime**.  This is CRITICAL!

PBKDF2 is used after scrypt so we can re-use master key and avoid calling scrypt every time we save a database during a session.  However, we still need the encryption keys to be unique every time (ChaCha20 breaks if you re-use key+nonce pairs), so we generate a fresh pbkdf2_salt every time we encrypt and use PBKDF2 to generate fresh keys using the unique salt and `master_key`.

Note that `scrypt_salt` will remain the same between saves.  It only changes if the user changes their password, at which point we generate a new `scrypt_salt` and new `master_key`.
