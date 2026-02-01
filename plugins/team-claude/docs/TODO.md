# Team Claude TODO Tracker

This document tracks incomplete features and known issues in the Team Claude plugin ecosystem.

## Incomplete Features

### Config Reading from YAML
- **File**: `cli/src/commands/server.ts:29`
- **Status**: Not implemented
- **Description**: Read server port from team-claude.yaml config instead of hardcoded default
- **Priority**: LOW
- **Context**: Currently uses DEFAULT_PORT constant. Implementation should parse `team-claude.yaml` in project root and extract server configuration.
- **Acceptance Criteria**: Server should load port from config file with fallback to DEFAULT_PORT if file missing

### Progress Bar Validation
- **File**: `cli/src/test/common.test.ts:568`
- **Status**: Test coverage gap
- **Description**: progressBar function needs input range validation (percent 0-100)
- **Priority**: LOW
- **Context**: Function works but doesn't validate bounds. Currently accepts invalid percentage values without error.
- **Acceptance Criteria**: Function should throw error or normalize values outside 0-100 range

### Server Streaming API Types
- **File**: `server/src/index.ts:153`
- **Status**: Type error
- **Description**: Hono's `c.streamText` method not found - needs proper typing or API update
- **Priority**: MEDIUM
- **Context**: TypeScript error: Property 'streamText' does not exist. May need Hono version update or different streaming approach.
- **Acceptance Criteria**: `npm run typecheck` passes without server errors

## Known Limitations

### Runtime Dependency
- CLI and server require Bun runtime (not Node.js compatible)
- See README for installation instructions
- Rationale: Bun provides superior TypeScript support and performance for this use case

### Review Functionality
- `tc review spec` and `tc review code` are placeholder implementations
- Currently return "SIMULATED" results pending agent integration
- **Status**: Awaiting integration with architect and code-reviewer agents
- **Expected Behavior**: Commands should analyze specs and code using dedicated agent systems

### CLI Test Coverage
- Some CLI commands have limited test coverage
- Integration tests may be needed for end-to-end workflows

## Completed Features

- Team Claude hook infrastructure
- Project session management (PSM)
- Magic keywords system
- tc CLI command structure
- Help documentation system

## Contributing

When adding new TODOs:

1. **Include file path and line number** - Helps developers locate the code
2. **Set priority** - Use HIGH/MEDIUM/LOW
   - HIGH: Blocks critical functionality
   - MEDIUM: Impacts user experience
   - LOW: Nice-to-have improvements
3. **Provide context** - Explain why this matters and what the impact is
4. **Add acceptance criteria** - Define what "done" means

### Format Template

```markdown
### Feature Name
- **File**: `path/to/file.ts:line-number`
- **Status**: Not implemented / In progress / Blocked
- **Description**: One-line summary
- **Priority**: HIGH / MEDIUM / LOW
- **Context**: Why this matters
- **Acceptance Criteria**: What needs to be true when complete
```

## Recent Changes

- Created TODO tracking document (2026-02-01)
- Identified config reading gap in server startup
- Identified progress bar validation gap in tests
