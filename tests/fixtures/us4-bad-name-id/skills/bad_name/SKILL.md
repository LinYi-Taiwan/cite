---
id: bad_name
name: Bad Name
description: An id with an underscore is path-safe but not a valid Agent Skills name slug.
---

# Bad Name

The `id` (`bad_name`) is emitted as the SKILL.md `name`, which the agent loader rejects
(underscores are not allowed). The build must fail rather than emit an unloadable skill.
