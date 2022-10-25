# Fortress Cryptography

This document describes the cryptography used by Fortress.

## Overview

There are two areas of Fortress where cryptography gets used.  On-disk encryption, and encryption of objects for transmission to and from sync backends.

Fortress uses the SIV construction for all encryption tasks.  The SIV construction is simple, deterministic, doesn't use randomness, and can use a stream cipher.  SIV and its security proof are provided in the citation (Deterministic authenticated-encryption).  Fortress is thus DAE secure.  The SIV construction requires a PRF-secure primitive, and an IND$-secure IV cipher.  In Fortress we use HMAC-SHA-512-256 as the PRF-secure primitive, and ChaCha20 wrapped by HMAC-SHA-512 as the IND$-secure cipher.

HMAC-SHA-512-256 (which is HMAC-SHA-512 truncated to 256-bits, not HMAC-SHA-512/256) is used because: it's faster on 64-bit platforms than HMAC-SHA-256; it is well seasoned, unlike potentially better functions like Blake, while still being fast enough; it's a random oracle according to the citation (Merkle-Damgård revisited); the HMAC construction has been shown to provide additional security when the underlying function fails, so it's a potentially more robust choice compared to SHA-512-256 even though SHA-512-256 has all the same properties.

ChaCha20 is wrapped by HMAC-SHA-512 using `HMAC-SHA-512(key, IV)` to derive a 256-bit key and 96-bit nonce for the invocation of ChaCha20.  Basically it turns ChaCha20 into a cipher with a 256-bit nonce.  This is used because the usual ChaCha20 cipher only accepts a 96-bit nonce, while our SIV implementation calls for 256-bits.  Reasons why we didn't use something else: XChaCha20 is a commonly used extension of ChaCha20 and derives its security straightforwardly from the XSalsa20 paper, however it only has a 192-bit nonce.  192-bits *might* be enough.  I would need to review the security proof for the SIV construction in-depth to know for sure how the security margin is affected by reducing the nonce space.  An XXChaCha20 primitive could be invented (three-layer cascade), but this requires studying the XSalsa20 security proof in depth to see if it covers the three-layer case.  Both options are likely secure, but require additional scrutiny (by myself and anyone reviewing Fortress's security).  In contrast we know for sure that HMAC-SHA-512 wrapped ChaCha20 fulfills the requirements and we already use HMAC-SHA-512 elsewhere.

SIV also calls for an Encode function, used to encode the input to the PRF.  It must be such that Encode uniquely encodes its inputs (given any input A, there exists no input B where `A!=B` and `Encode(A) = Encode(B)`).  Fortress simply uses `Encode(AAD, Plaintext) = AAD || Plaintext || le64encode(AAD.length) || le64encode(Plaintext.length)`.


### Citations

(Merkle-Damgård revisited) Coron, Jean-Sébastien, et al. "Merkle-Damgård revisited: How to construct a hash function." Annual International Cryptology Conference. Springer, Berlin, Heidelberg, 2005.

(Deterministic authenticated-encryption) Abbadi, Mohammad, et al. "Deterministic authenticated-encryption: A provable-security treatment of the keywrap problem." Journal of Applied Sciences 8.21 (1996): pp-1.



## Primitives

* HMAC-SHA-512
* HMAC-SHA-512-256
* HMAC-SHA-256 (needed by PBKDF2-SHA-256)
* ChaCha20
* scrypt
* PBKDF2-SHA-256 (needed by scrypt)
* SHA-512
* SHA-256 (needed by HMAC-SHA-256)



## Keys

1024-bit keys are used because keying material here is "free" and they are the exact size that HMAC-SHA-512 ends up using.

```
SivEncryptionKeys:
	* siv_key: 1024-bits
	* cipher_key: 1024-bits
```



## Functions

NOTE: `String` is meant to be a UTF-8 encoded string, so it's effectively equivalent to a [u8].

### SivEncrypt
`aad` is Additional Authenticated Data.  AAD is not included in the resulting ciphertext, but it is used as part of the authentication and thus SIV generation.  The same plaintext will encrypt differently if the AAD is different.

The returned SIV can be treated as a unique, deterministic identifier (ID) for the (aad, plaintext) pair.  The SIV does not need to be secret.

```
SivEncrypt (keys: SivEncyptionKeys, aad: [u8], plaintext: [u8]) -> ([u8; 32], [u8]) -> ([u8], [u8])
	mac_data = Encode (a=aad, b=plaintext)
	siv = HMAC-SHA-512-256 (key=keys.siv_key, data=mac_data)
	ciphertext = Cipher (key=keys.cipher_key, nonce=siv, data=plaintext)

	return siv, ciphertext
```

### SivDecrypt
```
SivDecrypt (keys: SivEncryptionKeys, siv: [u8; 32], aad: [u8], ciphertext: [u8]) -> [u8]
	plaintext = Cipher (key=keys.cipher_key, nonce=siv, data=ciphertext)
	mac_data = Encode (a=aad, b=plaintext)
	expected_siv = HMAC-SHA-512-256 (key=keys.siv_key, data=mac_data)
	assert!(constant_time_eq (siv, expected_siv))

	return plaintext
```

### PassphraseDerive
Deterministically derive keying material from a username+passphrase combo, using a hard KDF.

```
PassphraseDerive (username: String, passphrase: String, log_n: u8, r: u32, p: u32, length: u64) -> [u8]
	return scrypt (password=passphrase, salt=username, N=1<<log_n, r=r, p=p, dkLen=length)
```

### Cipher
`Cipher` is symmetrical; it is both the encryption and decryption function.  It behaves as an IND$-secure cipher with a 1024-bit key and 256-bit nonce.

```
Cipher (key: [u8; 128], nonce: [u8; 32], data: [u8])
	chacha_key, chacha_nonce = HMAC-SHA-512 (key, nonce).split (32)

	return ChaCha20 (chacha_key, chacha_nonce[:12], data)
```

### Encode
Uniquely encodes the AAD and plaintext for MAC calculation.

For all `A`, `B`, `C`, and `D` where `(A, B) != (C, D)` it is true that `Encode(A, B) != Encode(C, D)`.

```
Encode (a: [u8], b: [u8])
	return a || b || le64encode (a.length) || le64encode (b.length)
```


## On-disk Format (V2)

    header_string:  UTF-8 NULL terminated string ("fortress2\0")
    scrypt_log_n:   scrypt parameter (u8)
    scrypt_r:       scrypt parameter (u32 little endian)
    scrypt_p:       scrypt parameter (u32 little endian)
    scrypt_salt:    scrypt parameter (u8 * 32)
    siv:            SIV for the encrypted data (u8 * 32)
    payload:        The encrypted data (*)
    checksum:       SHA-512-256 of all proceeding data (u8 * 32)


`header_string` (e.g. fortress2) specifies the format version.  V2 only supports one encryption standard.  In future versions different encryption standards might be used, and in those cases the file format may be different.

During encryption, a set of scrypt parameters is chosen and fed along with the user's username and passphrase to `PassphraseDerive` to generate an `SivEncryptionKeys`.  The serialized database is then fed into `SivEncrypt` with no AAD to generate the SIV and encrypted payload.

During decryption, the scrypt parameters can be parsed from the file, keys can be re-derived using `PassphraseDerive`, and then the SIV and payload can be fed into `SivDecrypt` with no AAD to recover the plaintext (or determine that the passphrase is incorrect).

Checksum helps to catch cases of file corruption.

Because the encryption scheme used here is deterministic, it is safe to keep scrypt salt constant, which helps reduce the need for CSRNG data.  The salt's main purpose is to deter rainbow table attacks.  Fortress tends to refresh this salt only when the user changes their passphrase.


## Network Cryptography

Fortress Objects are encrypted end-to-end during the syncing process.  A fixed set of scrypt parameters is used for this, where `log_n=20`, `r=8`, `p=128`.  These parameters are specifically chosen to be aggressive, since risk of brute-force attack is higher for network traffic than for local storage.  The keys used for network encryption only need to be generated once, and then they can be cached locally inside the user's database, so taking 5 or more minutes to generate them the first time is not much of an inconvenience.

`HMAC-SHA-512` is used to deterministically derive a salt for the network keys from the user's username.  A fixed HMAC key (51c3d00bde2b3258ca179272153ed0fd2e475604da14bac2b7a3b9bcb0504fba) is used.  The use of the user's username as salt helps to deter rainbow tables.

The network keys are derived using `PassphraseDerive` with the aforementioned scrypt parameters, username salt, and the user's passphrase.  An `SivEncryptionKeys` is derived along with a 32-byte `LoginKey` which is used to authenticate the user to backend servers.

Finally, `HMAC-SHA-512` is used to deterministically derive a `LoginId` from the user's username.  This is used to identify the user to backend servers.  A fixed HMAC key (87650906efda47657a1f95368f7af711c0d10e514735443c0bdca46e1181aac4) is used.  For backend sync servers that don't need the user's real username/email, this helps protect their identity.

Database Objects are serialized and then encrypted using `SivEncrypt`, the previously derived `SivEncryptionKeys`, and the Object's 32-byte ID used as AAD.  Using the Object's ID as AAD prevents backend servers from mixing up objects.  The resulting combination of Object ID, SIV, and ciphertext can then be used to securely sync against a backend server.

Because this encryption scheme is deterministic, Fortress can easily determine if any Objects on a backend are different based on their SIV.  If SIV differs, then the local Object and the server Object must differ and should be sync'd.



## Cost of attacking user's passphrase

If an attacker gets ahold of a user's encrypted data they can use that to begin cracking the user's passphrase, either by brute force or dictionary attack.  (Note: Rainbow tables are unlikely because we salt using a (hopefully) unique username/email/id/etc).

In such a scenario it's useful to think in terms of how expensive it would be to attack a user's passphrase.  Since we use scrypt we know it'll be fairly expensive, but just how expensive?

First we need to know how complex the passphrase is.  Hopefully the user chooses a good passphrase, but most don't.  Microsoft did some research here and found that on average user passphrases have an estimated entropy of 40.54 bits.  (Let's ignore the usual caveats about password entropy for simplicity.)

Second we need to know how much it costs to run one scrypt attempt.  This can get complicated quickly.  Even though scrypt is a memory-hard KDF, like all memory-hard KDFs the attacker can choose to trade memory for compute.  And of course an attacker can use everything from commodity CPUs, to GPUs, to custom ASICs.

Luckily scrypt based cryptocurrencies exist and provide a direct monetary incentive to run scrypt at the lowest cost possible at large scales.  Using data from these cryptocurrency networks we can put a price tag on an scrypt hash and be reasonably sure that that price is the lowest available by any means today.

The caveat here is that the scrypt parameters used by these cryptocurrencies won't match the ones Fortress or anyone else uses.  Litecoin, for example, uses `N=2**10` (1MB RAM).  But it's possible to trade memory for compute, as mentioned, therefore we can extrapolate what it would be like to use Litecoin ASIC-like hardware to attack Fortress's encryption.

The extrapolation can be derived from scrypt's Big-Omega, which is `N**2`.  If a machine uses no memory to its advantage it can compute any scrypt function using on the order of `N**2` Salsa20 hashes.  Since this is Big-Omega it omits some constants and factors.  We'll add some of those back in and use `r * p * N**2`.  (Keep in mind that this is still "on the order of" and is still missing a few factors.)

Litecoin uses `N=2**10, r=1, p=1`.  As of May 2021 the cost of a single Litecoin hash is `8.05e-14 USD` (derived using Litecoin's current difficulty and average block reward).  Using the previous formula we can say that `8.05e-14 USD` buys us `1 * 1 * 2**10 * 2**10` Salsa hashes.  So the cost of a Salsa hash is approximately `7.68e-20 USD`.

Now we can build a formula to convert an arbitrary set of scrypt parameters into passphrase cracking cost.  We can calculate how many Salsa hashes are needed using our previous formula: `r * p * N**2`.  And we know that the average passphrase has ~40 bits of entropy and thus requires on average `2**39` attempts to crack.  Thus on average it would take `r * p * N**2 * 2**39` Salsa hashes to crack a passphrase.  Given the cost of a Salsa hash we can finally say that, on average, it costs:

`r * p * N**2 * 2**39 * 7.68e-20`

$USD to crack a passphrase protected by scrypt.  For Fortress network objects that works out to `8 * 128 * 2**20 * 2**20 * 2**39 * 7.68e-20` which is ~$47,000,000 USD.

Thus, as of today a sophisticated attacker with a huge amount of capital invested in custom hardware would still have to spend on the order of $47 million USD to crack a single Fortress protected by an average passphrase.

The point of the former exercise was to demonstrate exceptional security even to the average user with an average passphrase.  Users are of course encouraged to use decent passwords, which are likely to exceed the average 40 bits of entropy.  A 60 bit password, for example, would cost $49 trillion USD to crack.