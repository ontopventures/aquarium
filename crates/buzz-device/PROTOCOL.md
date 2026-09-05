# Aquarium device command protocol v1

Separate from NIP-AO (24200), NIP-AB (24134), and unused job kinds 43001–43006.

| Kind | Role |
|---|---|
| 30180 | Device advertisement (parameterized replaceable) |
| 30181 | Device grant projection (parameterized replaceable; local grant file is execution authority) |
| 43200 | Device request, NIP-44 to device pubkey, p-gated |
| 43201 | Device receipt, NIP-44 to actor pubkey, p-gated |

Operations: `inspect_capabilities`, `create_checkout`, `inspect_request`, `start_session`, `cancel_session`.

Request id: `{13-digit-ms}-{32 hex}`. Max age 24h, future skew 5min. Fingerprint is SHA-256 of canonical `{op, device_id, grant_generation, params}`.

The initiating client never executes git or agents. Wrong-host fallback is forbidden.
