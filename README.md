# estimate

[![CI](https://github.com/float3/estimate/actions/workflows/ci.yml/badge.svg)](https://github.com/float3/estimate/actions/workflows/ci.yml)

The estimation game for a room of people, at
[estimate.rationality-munich.com](https://estimate.rationality-munich.com).

You open a room and put the join link on the wall. Everyone scans it and
gives a name. You ask a question with a number for an answer, and everyone
sends the range they are ninety per cent sure it falls in. You get the
median bounds, the average bounds, the lowest and highest anyone went, the
widest range and the narrowest, and, once you enter the answer, how many
of the ranges contained it.

The room then discusses the question and you ask for a second range. Each
phone starts from that player's first answer, so submitting it unchanged
counts as an answer. The second pass is what makes the first one worth
collecting: everybody sees how far the discussion moved them, and the room
sees whether the second answers were closer to the truth.

Built on [rooms](https://github.com/float3/rooms).

## What it reports

A range is a claim about how much you know: at ninety per cent, nine in ten
of your ranges should contain the answer. The scoreboard counts how often
somebody was right and how often they were narrower than the room, so that
a hit rate cannot be bought with very wide ranges.

## The two passes

A question goes through four stages, and the host moves it on:

| | |
| --- | --- |
| First ranges | taking everybody's first answer |
| Talking it over | nothing is being taken; the room argues |
| Second ranges | taking it again, prefilled with the first |
| Revealed | the answer, the room's numbers, and what the discussion did |

The host can go back a stage for somebody who was out of the room, and can
reveal from any stage. A question revealed straight from the first pass has
no second pass. A two-pass question reports who changed their range, who
narrowed and who widened, how far the median midpoint moved, whose moved
furthest, whose first answer the rest of the room moved towards, and how
many ranges contained the answer in each pass.

Influence is measured per person: take their first midpoint as a fixed
point, and average how much closer everybody else's midpoint got to it
between the two passes. Their own movement is left out of their own
score, so nobody scores by moving.

Nothing of the first pass is shown to players during the discussion. The
host sees it and can read out as much of it as they like; putting the
spread on everybody's phone would anchor the second answer to it.

## Three links

| | |
| --- | --- |
| `/h/<key>` | runs the room. Keep it; it is the only way back in. |
| `/j/<key>` | joins it. This is the one on the wall. |
| `/p/<key>` | one player's own, kept by their phone. |

The key in the link is the only access control: 24 random characters, and
no accounts. Anyone with the host link can run the room. That is
appropriate for a group of people sitting together and not for anything
else; see [rooms](https://github.com/float3/rooms) for what it does and
does not protect.

The host sees everything. A player is sent their own two ranges and how
many people have answered. Until the host reveals the question they are
not sent the answer, anybody else's bounds, or a tally of their own hits,
which would tell them whether their range had contained it.

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

The page is TypeScript in `client/`, compiled into the binary by the build
script with [rooms-client](https://github.com/float3/rooms). `esbuild`
bundles it and `tsc` checks it, both from the dev shell. No JavaScript is
written by hand here.
