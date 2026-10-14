# estimate

[![CI](https://github.com/float3/estimate/actions/workflows/ci.yml/badge.svg)](https://github.com/float3/estimate/actions/workflows/ci.yml)

the estimation game for a room of people, at
[estimate.rationality-munich.com](https://estimate.rationality-munich.com).

you open a room and put the join link on the wall. everyone scans it and
gives a name. you ask a question with a number for an answer, and everyone
sends the range they are ninety per cent sure it falls in. you get the
median bounds, the average bounds, the lowest and highest anyone went, the
widest range and the narrowest, and, once you enter the answer, how many
of the ranges contained it.

the room then discusses the question and you ask for a second range. each
phone starts from that player's first answer, so submitting it unchanged
counts as an answer. the second pass is what makes the first one worth
collecting: everybody sees how far the discussion moved them, and the room
sees whether the second answers were closer to the truth.

built on [rooms](https://github.com/float3/rooms).

## what it reports

a range is a claim about how much you know: at ninety per cent, nine in ten
of your ranges should contain the answer. the scoreboard counts how often
somebody was right and how often they were narrower than the room, so that
a hit rate cannot be bought with very wide ranges.

## the two passes

a question goes through four stages, and the host moves it on:

| | |
| --- | --- |
| First ranges | taking everybody's first answer |
| Talking it over | nothing is being taken; the room argues |
| Second ranges | taking it again, prefilled with the first |
| Revealed | the answer, the room's numbers, and what the discussion did |

the host can go back a stage for somebody who was out of the room, and can
reveal from any stage. a question revealed straight from the first pass has
no second pass. a two-pass question reports who changed their range, who
narrowed and who widened, how far the median midpoint moved, whose moved
furthest, whose first answer the rest of the room moved towards, and how
many ranges contained the answer in each pass.

influence is measured per person: take their first midpoint as a fixed
point, and average how much closer everybody else's midpoint got to it
between the two passes. their own movement is left out of their own
score, so nobody scores by moving.

nothing of the first pass is shown to players during the discussion. the
host sees it and can read out as much of it as they like; putting the
spread on everybody's phone would anchor the second answer to it.

## three links

| | |
| --- | --- |
| `/h/<key>` | runs the room. Keep it; it is the only way back in. |
| `/j/<key>` | joins it. This is the one on the wall. |
| `/p/<key>` | one player's own, kept by their phone. |

the key in the link is the only access control: 24 random characters, and
no accounts. anyone with the host link can run the room. that is
appropriate for a group of people sitting together and not for anything
else; see [rooms](https://github.com/float3/rooms) for what it does and
does not protect.

the host sees everything. a player is sent their own two ranges and how
many people have answered. until the host reveals the question they are
not sent the answer, anybody else's bounds, or a tally of their own hits,
which would tell them whether their range had contained it.

## running it

```sh
nix run github:float3/estimate
```

| | |
| --- | --- |
| `ESTIMATE_ADDR` | what to listen on. `127.0.0.1:8097`. |
| `ESTIMATE_STATE` | where the rooms are kept. `rooms.json`. |

the NixOS module takes a port and the domains to serve on:

```nix
services.estimate = {
  enable = true;
  port = 8097;
  domains = ["estimate.example.com"];
};
```

rooms are forgotten a day after anyone last looked at one.

## development

```sh
nix develop --command cargo test
```

the page is TypeScript in `client/`, compiled into the binary by the build
script with [rooms-client](https://github.com/float3/rooms). `esbuild`
bundles it and `tsc` checks it, both from the dev shell. no JavaScript is
written by hand here.
