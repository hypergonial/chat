# Preferences

## Overview

The preferences object is used to store and retrieve user preferences. It is up to implementing clients to support and implement these features. It currently supports & stores the following values:

| Key | Type | Description |
| --- | --- | --- |
| `flags` | `int` | The user's preference flags. |
| `message_grouping_timeout` | `int` | The amount of time in seconds for clients to group messages together from the same author, if they support it. (Default `60`) |
| `layout` | `int` | The user's preferred layout. (Default `1`) |
| `text_size` | `int` | The user's preferred text size. (Default `12`) |
| `locale` | `string` | The user's preferred locale. Max length of `5`. (Default `en_US`) |

## Flags

The preference flags are stored as a bitfield. The following flags are currently defined:

| Flag | Value | Description |
| --- | --- | --- |
| `RENDER_ATTACHMENTS` | `1` | Whether or not the client should render attachment previews. (Default `true`) |
| `AUTOPLAY_GIF` | `1 << 1` | Whether or not the client should autoplay embedded GIFs. (Default `true`) |

> Note: More flags may be added in the future, this list is non-exhaustive.

## Layouts

The following layouts are currently defined:

| Layout | Value | Description |
| --- | --- | --- |
| `COMPACT` | `0` | The compact layout. |
| `NORMAL` | `1` | The normal layout. (Default) |
| `COMFY` | `2` | The "comfy" layout. |

> Note: It is up to the client to decide how to render these layouts.

## Example payload

```json
{
  "flags": 3,
  "message_grouping_timeout": 60,
  "layout": 1,
  "text_size": 12,
  "locale": "en_US"
}
```
