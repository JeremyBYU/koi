# cli-game

cli-game is a tranquil game that runs in the terminal: a top-down view of a
koi pond, animated at 60 frames per second, with music playing while it runs.
It is something calm to leave open next to work. Nothing here is decided yet;
this file holds the idea and the questions to answer before building.

## The idea

- A koi pond seen from above. The koi swim around on their own.
- The water moves. Food landing on the surface makes ripples, and fish leave
  wakes. Some real physics, not just drawn waves.
- You can feed the koi, and they come to the food.
- Maybe turtles too.
- Smooth motion at 60 fps. It should feel calm, never busy.
- A beautiful, calming color palette. Clear and readable, not murky.
- Music playing alongside it. Ideally it connects to my YouTube account and
  plays my own playlists.
- It runs in Ghostty, which is fast and supports images, so the game can use
  more than plain text characters if that looks better.
- Later, some tie-in with muster. What that means is still open.

## Questions to answer first

These are for research agents before any code is written: technology, look,
and what is possible at all.

| Question | What I know so far |
|---|---|
| Which language and libraries? | Go is usually very good for CLIs. Whether it suits a 60 fps animated game is the question; compare it with the alternatives. |
| How do you draw at 60 fps in a terminal? | Two broad options: colored text cells (for example half-block characters with 24-bit color), or real images through the graphics protocol Ghostty supports. What each looks like, and how smooth each can get, is unknown. |
| How should the water work? | Ripples and wakes need some kind of water simulation. What is cheap enough to run every frame and still looks good? |
| How should the koi move? | They need to look alive: schooling, turning, drifting, rushing to food. |
| What palette? | Calm and beautiful, with enough contrast to stay clear. Needs a few candidate palettes to compare side by side. |
| Can it play my YouTube playlists? | I don't know whether streaming music from a YouTube account into a CLI is possible, or allowed. Needs checking, along with fallbacks. |
| How do I feed the fish? | Mouse click on the pond, keyboard, or both. |

## How I want to start

Brainstorm first, in parallel: the technology, the color palettes, and what
the pond could look like. Then build a small prototype that answers the
60 fps question before building the rest.
