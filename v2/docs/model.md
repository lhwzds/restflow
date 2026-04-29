# model

Model owns provider and model identity for the V2 kernel.

Path: `crates/model`

Owns:
- provider identity
- model identity
- canonical model construction

Must Not:
- read secrets
- call model providers
- depend on daemon state

Inputs:
- provider IDs
- model IDs

Outputs:
- Model
- Provider

Used By:
- agent
- auth
- skill

Verify:
- cargo check -p model

