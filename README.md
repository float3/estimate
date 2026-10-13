# estimate

[![CI](https://github.com/float3/estimate/actions/workflows/ci.yml/badge.svg)](https://github.com/float3/estimate/actions/workflows/ci.yml)

The estimation game for a room of people, at
[estimate.rationality-munich.com](https://estimate.rationality-munich.com).

You open a room and put the join link on the wall. Everyone scans it and
gives a name. You ask a question with a number for an answer; everyone
sends the range they are ninety per cent sure it falls in. You see what the
room thinks — the median bounds, the average bounds, how low and how high
anyone went, the widest range and the tightest — and once you type the
answer in, how many of the ranges contained it.

Built on [rooms](https://github.com/float3/rooms).

## Why ranges

A single guess is almost always wrong, and knowing it was wrong tells you
nothing. A range is a claim about how much you know: at ninety per cent,
nine in ten of your ranges ought to contain the answer. Fewer than that and
you are overconfident, which is the usual result. Ten out of ten and your
ranges are wider than they need to be — which is why the scoreboard counts
how often somebody came in tighter than the room as well as how often they
were right.

## Three links

| | |
| --- | --- |
| `/h/<key>` | runs the room. Keep it; it is the only way back in. |
| `/j/<key>` | joins it. This is the one on the wall. |
| `/p/<key>` | one player's own, kept by their phone. |

The key in the link is the whole access model: 24 random characters, no
accounts. Anyone with the host link runs the room. That is the right amount
of security for a room of people looking at the same wall and no more than
that — see [rooms](https://github.com/float3/rooms) for what that means.

The host sees everything. A player is sent their own range, how many people
have answered, and nothing else until the host reveals the question — not
the answer, not anybody else's bounds, not even a tally of their own hits,
which would say whether their range had contained it.

## Running it

```sh
nix run github:float3/estimate
```

| | |
| --- | --- |
| `ESTIMATE_ADDR` | what to listen on. `127.0.0.1:8097`. |
| `ESTIMATE_STATE` | where the rooms are kept. `rooms.json`. |

The NixOS module takes a port and the domains to serve on:

```nix
services.estimate = {
  enable = true;
  port = 8097;
  domains = ["estimate.example.com"];
};
```

Rooms are forgotten a day after anyone last looked at one.

## Development

```sh
nix develop --command cargo test
```
