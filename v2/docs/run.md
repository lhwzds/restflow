# run

Run owns durable task and run execution concepts.

Path: `crates/run`

Owns:
- Task
- Run
- run status
- durable execution vocabulary

Must Not:
- become a second agent loop
- own skill catalog
- create a separate team runtime

Inputs:
- task definitions
- agent execution events
- checkpoint state
- skill catalog

Outputs:
- run status
- run history
- run artifacts
- agent run input

Depends On:
- agent
- chat
- event
- skill
- store

Verify:
- cargo check -p run

