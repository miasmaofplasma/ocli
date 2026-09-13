---
Created: "{{DATE:YYYY-MM-DD HH:mm}}"
aliases:
  - "{{VALUE:FeatureType}}-{{VALUE:TicketNumber}}"
type: "{{VALUE:FeatureType}}"
description:
owner:
epic:
relates-to:
blocked-by:
sprint:
repo:
jira: https://bddevops.atlassian.net/browse/{{VALUE:FeatureType}}-{{VALUE:TicketNumber}}
pr:
estimate:
status:
done: false
tags:
  - feature
  - codewaves
---
 Owner: `INPUT[suggester(optionQuery("notes/people")):owner]` |  Ticket Type: `INPUT[inlineSelect(option(BCP), option(NGP), option(Ticket), option(Epic)):type]` |  Estimate: `INPUT[inlineSelect(option(1), option(2), option(3), option(5), option(8)):estimate]` 
 Done: `INPUT[toggle:done]` 
 
 ---
# Description

# Acceptance Criteria

# Dev Notes

## Progress
## Notes
## Descisions

## Open Questions

<!-- ocli:footer -->
--- 

# Relates to 
```meta-bind
INPUT[listSuggester(optionQuery("notes")):relates-to]
```

# Blocked by 
```meta-bind
INPUT[listSuggester(optionQuery("notes")):blocked-by]
```
