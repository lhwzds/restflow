# store

Store owns backend-neutral repository contracts.

Path: `crates/store`

Owns:
- Repository trait
- memory repository
- get/list/put/delete repository contract
- backend abstraction boundary

Must Not:
- own business decisions
- expose backend handles to runtime modules
- store UI overlay state

Inputs:
- record IDs
- typed records

Outputs:
- persisted records
- record lists
- deletion status

Used By:
- auth
- chat
- run

Verify:
- cargo check -p store

