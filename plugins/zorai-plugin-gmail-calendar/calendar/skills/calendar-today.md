---
name: calendar
description: >
  Full Google Calendar integration — list events, view details, create,
  update, delete events, and list calendars.
---

# Calendar Plugin

You have access to the **Calendar plugin** with these endpoints:

| Endpoint | Method | What it does |
|----------|--------|-------------|
| `list_events` | GET | List events in a time range (requires time_min, time_max) |
| `list_calendar_events` | GET | Alias for listing events with Pack 1 naming |
| `get_event` | GET | Get full event details including attendees |
| `create_event` | POST | Create a new event |
| `update_event` | PATCH | Change only the given fields of an event |
| `update_attendance` | PATCH | Answer an invitation (accepted, tentative, declined) |
| `delete_event` | DELETE | Delete an event |
| `list_calendars` | GET | List all user's calendars |

## Listing Events

**IMPORTANT:** You must compute RFC3339 date boundaries. For today (e.g., 2026-03-25):
- `time_min`: `2026-03-25T00:00:00Z`
- `time_max`: `2026-03-26T00:00:00Z`

For a full week, set time_max 7 days ahead.

```json
{"plugin_name": "calendar", "endpoint_name": "list_calendar_events", "params": {"time_min": "2026-03-25T00:00:00Z", "time_max": "2026-03-26T00:00:00Z"}}
```

`list_events` remains supported as an equivalent alias.

Present as numbered list: **10:00-11:00** — Event Title / Location: ...

## Event Details

```json
{"plugin_name": "calendar", "endpoint_name": "get_event", "params": {"event_id": "EVENT_ID"}}
```

Returns summary, times, location, description, creator, organizer, and attendees with RSVP status.

## Creating Events

**IMPORTANT:** Always pass ALL params including `location` and `description` (use empty string `""` if not specified by user).

```json
{"plugin_name": "calendar", "endpoint_name": "create_event", "params": {
  "summary": "Team standup",
  "start_time": "2026-03-26T10:00:00+01:00",
  "end_time": "2026-03-26T10:30:00+01:00",
  "location": "Conference Room A",
  "description": "Daily sync"
}}
```

Use the user's timezone offset if known, otherwise UTC.

## Updating Events

Pass `event_id` and a `patch` object with only the Google Calendar event fields to change; everything else (attendees, reminders, conference link) stays as it is:
```json
{"plugin_name": "calendar", "endpoint_name": "update_event", "params": {
  "event_id": "EVENT_ID",
  "patch": {
    "summary": "Updated title",
    "start": {"dateTime": "2026-03-26T11:00:00+01:00"},
    "end": {"dateTime": "2026-03-26T11:30:00+01:00"}
  }
}}
```

## Answering Invitations

```json
{"plugin_name": "calendar", "endpoint_name": "update_attendance", "params": {"event_id": "EVENT_ID", "email": "YOUR_EMAIL", "response_status": "accepted"}}
```

Use `accepted`, `tentative` or `declined`.

## Deleting Events

Deleting cannot be undone; confirm with the user first unless they asked for it explicitly.

```json
{"plugin_name": "calendar", "endpoint_name": "delete_event", "params": {"event_id": "EVENT_ID"}}
```

## Listing Calendars

```json
{"plugin_name": "calendar", "endpoint_name": "list_calendars", "params": {}}
```

Use a specific calendar by passing `calendar_id` to any endpoint.

## Error Handling

If not connected: direct user to **Settings > Plugins > Calendar** to configure OAuth.
If auth error: token may have expired — daemon auto-refreshes, or ask user to reconnect.
