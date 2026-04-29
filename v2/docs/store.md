# store

Store owns backend-neutral repository contracts.

Path: `crates/store`

Owns:
- Store trait
- get/put/delete repository contract
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
- deletion status

Used By:
- auth
- chat
- run

Verify:
- cargo check -p store

