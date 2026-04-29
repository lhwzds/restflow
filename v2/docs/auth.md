# auth

Auth owns secret references and provider access profiles.

Path: `crates/auth`

Owns:
- SecretRef
- Profile
- provider credential references

Must Not:
- expose secret values in docs
- define model catalog entries
- call UI code

Inputs:
- provider IDs
- secret keys

Outputs:
- auth profiles
- secret references

Depends On:
- model
- store

Verify:
- cargo check -p auth

