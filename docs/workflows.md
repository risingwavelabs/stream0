# Workflows

Agent workflows are a new top-level product surface in Box0. They are inspired by tools like n8n and Dify, but the first version should stay tightly focused on Box0's strength: orchestrating named agents.

This document defines the V1 product and technical direction.

## Summary

- Add a new `Workflows` tab in the dashboard.
- Model workflows as first-class objects, independent from `tasks`.
- Treat workflows as agent-first DAGs.
- Optimize V1 for visibility and light editing, not full low-code automation.
- Require at least one existing agent in the workspace before a workflow can be created.
- Do not add third-party integrations in V1.

## Why Box0 needs workflows

Box0 already has strong primitives for agent execution:

- named agents
- inbox-based task dispatch
- thread history
- workspace scoping
- web visibility

What is missing is a durable, user-visible way to describe how multiple agents should collaborate on a task.

Without workflows, users can still ask one agent to plan and execute steps internally. That is useful, but it is not enough for:

- explicit agent-to-step assignment
- reusable multi-agent patterns
- visible dependencies between steps
- partial reruns
- pausing for human input
- explaining how a result was produced

## Product positioning

### Agent vs plan vs workflow

These should be treated as different concepts:

| Concept | Meaning |
|---|---|
| Agent | A reusable executor with a role, instructions, runtime, and machine placement |
| Plan | A temporary execution outline, often created by an agent for one run |
| Workflow | A user-visible, editable, reusable execution graph |

Practical rule:

- if the steps only help one execution succeed, they are a plan
- if the steps should be visible, editable, assigned, and reused, they are a workflow

### Workflow vs task

`tasks` should not be the foundation for workflows.

Tasks are a legacy execution surface and may be removed later. Workflows should be introduced as a new independent model with their own definitions and run history.

### Workflow vs future kanban

A future kanban item should represent "what needs to be done."

A workflow represents "how this kind of work should be executed."

One kanban item may run without a workflow, may use a temporary plan, or may attach to a reusable workflow definition.

## Product principles

### 1. Agent-first

The core value of Box0 workflow is not generic step sequencing. It is explicit coordination between agents.

Single-agent multi-step flows are supported, but they are not the main product story.

### 2. DAG first, but small

The execution model should be a DAG from the start, but V1 should support only a minimal subset of node types and rules.

### 3. Show before automate

V1 should prioritize:

- clear structure
- visible dependencies
- understandable execution state
- safe editing

It should not try to match the full automation surface of n8n.

### 4. Light editing over full visual programming

Users should be able to:

- add and remove steps
- choose an agent for a step
- edit step instructions
- connect steps with dependencies

V1 should avoid heavy features like:

- loops
- arbitrary scripting
- complex variable templates
- dozens of node types
- rich external integrations

## V1 scope

### In scope

- new `Workflows` tab in the web UI
- workflow list page
- workflow empty state
- workflow detail page with graph and form-based editing
- workflow creation only when the workspace has at least one agent
- draft workflow definitions
- workflow DAG validation
- workflow runs
- per-step execution state
- manual rerun of a failed or completed step run
- human-input pause nodes

### Out of scope

- third-party integrations
- loops
- nested workflows
- dynamic code nodes
- variable mapping UI
- arbitrary conditional expressions
- public workflow templates marketplace
- replacing all current task UI in the same milestone

## V1 node types

V1 should support only four node types:

### `start`

- exactly one per workflow
- no incoming edges
- provides run input

### `agent`

- bound to one existing agent in the workspace
- contains a title and prompt
- may have multiple upstream dependencies
- creates one execution unit in a workflow run

### `human_input`

- pauses the workflow
- asks the user for input or approval
- resumes downstream nodes after input is submitted

### `end`

- optional in V1, but recommended for clarity
- no outgoing edges
- marks terminal completion

## DAG rules

V1 should keep the rules simple and explicit:

- a workflow must contain exactly one `start`
- a workflow must contain at least one `agent` node
- a workflow must be acyclic
- every node except `start` must be reachable from `start`
- `end` cannot have outgoing edges
- `start` cannot have incoming edges
- `agent` and `human_input` nodes may fan out to multiple downstream nodes

The backend should reject invalid definitions on save and publish.

## UX

### Workflow list

Add a `Workflows` tab alongside the existing top-level surfaces.

List view should show:

- workflow name
- status: `draft` or `published`
- number of nodes
- number of agents used
- updated time
- last run result

### Empty states

If the workspace has zero agents:

- show an empty state explaining that workflows require at least one agent
- provide a clear CTA to create an agent first

If the workspace has agents but no workflows:

- show a CTA to create the first workflow

### Create workflow

V1 create flow can be lightweight:

1. click `Create Workflow`
2. enter name and optional description
3. start with a default graph:
   - `Start`
   - one `Agent Step`
   - `End`
4. select an agent for the first step
5. save as `draft`

### Workflow detail and editor

The main workflow page should be display-first:

- left or center: graph view
- right side: selected node inspector
- top actions: save, run, publish, archive

Editing should be form-driven even if the graph is visual.

Users should be able to:

- rename the workflow
- add a node
- delete a node
- edit node title
- edit node prompt
- choose the bound agent
- connect and disconnect nodes

### Execution visibility

When a workflow runs, users should be able to see:

- run status
- which nodes are pending, running, blocked, waiting for input, done, or failed
- which agent each step was assigned to
- node outputs
- failure reason

This visibility is a major part of the product value.

## Execution model

The runtime model should separate definition from execution:

| Object | Purpose |
|---|---|
| Workflow | A saved DAG definition |
| Workflow run | One execution of a workflow |
| Step run | One execution of one node within a workflow run |

### Run lifecycle

Suggested workflow run states:

- `queued`
- `running`
- `waiting_for_input`
- `done`
- `failed`
- `cancelled`

Suggested step run states:

- `pending`
- `ready`
- `running`
- `waiting_for_input`
- `done`
- `failed`
- `skipped`

### Scheduling

At runtime:

1. create a workflow run
2. create step runs for all nodes
3. mark `start` done immediately with the run input
4. find all nodes whose dependencies are satisfied
5. mark them `ready`
6. dispatch `agent` nodes
7. pause on `human_input`
8. when a node completes, reevaluate downstream nodes
9. complete the run when all reachable terminal nodes complete

Independent ready nodes may run in parallel.

### Agent step execution

An `agent` node should execute by creating a thread for the bound agent and sending a request through the same inbox-based mechanism already used elsewhere in Box0.

Each step run should keep:

- workflow run id
- node id
- bound agent name
- thread id
- input payload
- output payload
- status
- error
- timestamps

### Input and output contract

V1 should keep data passing intentionally simple.

Each node receives:

- workflow run input
- upstream node outputs as plain text blocks
- the node's own prompt

The backend can compose a structured prompt like:

```text
Workflow: <workflow name>
Step: <step title>

Run input:
<input>

Upstream outputs:
<node A output>
<node B output>

Step instructions:
<node prompt>
```

This is enough for V1 and avoids the complexity of a full variable mapping language.

### Human input

A `human_input` node should:

- surface a prompt in the UI
- pause the workflow run
- accept a user response
- store the response as the node output
- unblock downstream nodes

## Data model

The workflow system should use new tables rather than reusing `tasks`.

### `workflows`

- `id`
- `workspace_name`
- `name`
- `description`
- `status` (`draft`, `published`, `archived`)
- `created_by`
- `created_at`
- `updated_at`

### `workflow_nodes`

- `id`
- `workflow_id`
- `kind` (`start`, `agent`, `human_input`, `end`)
- `title`
- `prompt`
- `agent_name` nullable
- `position_x`
- `position_y`
- `created_at`
- `updated_at`

### `workflow_edges`

- `id`
- `workflow_id`
- `source_node_id`
- `target_node_id`
- unique constraint on `(workflow_id, source_node_id, target_node_id)`

### `workflow_runs`

- `id`
- `workflow_id`
- `workspace_name`
- `status`
- `input`
- `started_by`
- `started_at`
- `finished_at`
- `error`

### `workflow_step_runs`

- `id`
- `workflow_run_id`
- `node_id`
- `agent_name` nullable
- `thread_id` nullable
- `status`
- `input`
- `output`
- `error`
- `started_at`
- `finished_at`

## API sketch

Suggested workspace-scoped endpoints:

```text
GET    /workspaces/{workspace}/workflows
POST   /workspaces/{workspace}/workflows
GET    /workspaces/{workspace}/workflows/{workflow_id}
PUT    /workspaces/{workspace}/workflows/{workflow_id}
DELETE /workspaces/{workspace}/workflows/{workflow_id}

POST   /workspaces/{workspace}/workflows/{workflow_id}/publish
POST   /workspaces/{workspace}/workflows/{workflow_id}/runs

GET    /workspaces/{workspace}/workflow-runs
GET    /workspaces/{workspace}/workflow-runs/{run_id}
POST   /workspaces/{workspace}/workflow-runs/{run_id}/steps/{step_run_id}/retry
POST   /workspaces/{workspace}/workflow-runs/{run_id}/steps/{step_run_id}/input
```

## Frontend implementation notes

The current frontend is a legacy dashboard mounted through Vue. V1 should keep the implementation lightweight and incremental.

Recommended approach:

- add a `Workflows` route and sidebar entry
- implement the first version in the existing `legacy-dashboard.js`
- keep the editor mostly form-based
- use a simple graph renderer instead of a heavy low-code canvas from day one

The graph can start as:

- node cards positioned on a canvas
- SVG lines between nodes
- click-to-select node
- inspector panel for edits

This is enough to validate the product before investing in a more advanced editor.

## Rust suitability

Rust is a strong fit for the backend of this feature.

Why:

- workflow execution is a state machine
- DAG validation is deterministic and type-friendly
- parallel step scheduling benefits from Rust's async runtime
- long-running orchestration needs stability
- the existing Box0 backend, database layer, and dispatch model are already in Rust

The hard part of a workflow product is more likely to be the editor UX than the Rust runtime.

## Rollout plan

### Milestone 1: definition and visibility

- add workflow tables
- add workflow CRUD
- add `Workflows` tab
- add list and detail views
- save draft workflows
- validate DAG structure

Success criteria:

- users can create and inspect workflows
- users can assign agents and dependencies
- users can understand the graph without running it

### Milestone 2: basic execution

- create workflow runs
- execute `agent` nodes
- execute fan-out when dependencies are satisfied
- show run status and step status
- allow retry on failed steps

Success criteria:

- a saved workflow can run end-to-end
- users can observe progress and outputs

### Milestone 3: human-in-the-loop

- add `human_input` nodes
- pause and resume runs from the UI

Success criteria:

- workflows can safely wait for approval or clarification

## Open questions

- Should `published` workflows be immutable except through a new version, or can they be edited in place?
- Should a workflow run snapshot the full workflow definition so later edits do not affect past runs?
- Should the same agent be allowed on many parallel nodes with no additional throttling?
- Should `end` remain optional in V1, or should it be required for simpler mental models?
- When a node has multiple upstream nodes, should their outputs be concatenated in edge order or node creation order?

## Recommended decision

For Box0 V1, workflows should be:

- a new top-level tab
- independent from legacy tasks
- agent-first DAGs
- display-first with light editing
- minimal in node types
- simple in data passing
- explicitly designed for future execution, not just mock display

That gives Box0 a workflow model that is credible, useful, and realistic to build without overreaching into full n8n-style automation on day one.
