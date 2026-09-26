# How the koi should move

## Recommendation

Split each koi into two layers that know nothing about each other's internals:

1. **Brain (a point with a velocity).** Reynolds steering behaviors: smooth wander, soft wall containment, separation, and weak alignment and cohesion so the fish form a loose shoal instead of a tight school. A small state machine on top handles food: cruise, notice, approach, eat, linger.
2. **Body (a spine chain).** The head follows the brain point. The other joints follow at a fixed distance with an angle limit per joint, as in Argonaut's procedural fish. A traveling sine wave is added at draw time for the tail beat, with frequency derived from swimming speed so slow fish beat their tails slowly.

Calm comes from three things more than any single number: **low speeds** (cruise around 0.4 body lengths per second), **smoothed steering** (desired velocity is low-pass filtered, so no fish ever snaps to a new heading), and **burst-and-coast gait** (a short tail-beat push, then a long glide), which real koi use. Turtles reuse the brain with slower numbers and replace the spine with a rigid shell plus paddling flippers.

Everything below fits in one Rust module per species. With 10 to 30 fish, a plain O(N²) neighbor loop is fine; no spatial hash is needed.

## Units and timing

- Measure everything in **body lengths (BL)** and **seconds**. A fish's length in pixels depends on the renderer (half-block cells or Kitty graphics), so keep the simulation in float world units and scale at draw time. This also means a resize or a switch of renderer does not change behavior.
- Run the simulation on a **fixed timestep** (dt = 1/60 s) with an accumulator, and render whatever the terminal manages. Under tmux, frames can be delayed or dropped by the multiplexer; a fixed timestep keeps fish speed identical whether the screen gets 60 fps or 30.
- Every rate is "per second × dt". For smoothing, use the frame-rate-independent form `alpha = 1 - exp(-dt / tau)`, never a fixed lerp factor per frame.
- Seed the RNG. With a seed, a pond replays identically, which makes tuning and tests much easier.

## The brain: steering behaviors

Reynolds' 1999 paper defines the behaviors used here: seek, flee, arrive, wander, containment, and the flocking trio of separation, alignment and cohesion. Each behavior returns a desired velocity; steering force is `desired - velocity`, clamped to a maximum force. ([Reynolds, Steering Behaviors for Autonomous Characters](https://www.red3d.com/cwr/papers/1999/gdc99steer.pdf), [red3d.com/cwr/steer](https://www.red3d.com/cwr/steer/), [Nature of Code ch. 5](https://natureofcode.com/autonomous-agents/))

### Per-frame update

```
for fish in fishes:
    steer = 0
    steer += w_wander   * wander(fish)
    steer += w_contain  * contain(fish, pond_sdf)
    steer += w_separate * separate(fish, neighbors)
    steer += w_align    * align(fish, neighbors)
    steer += w_cohere   * cohere(fish, neighbors)
    steer += w_avoid    * avoid(fish, turtles)
    steer += w_goal     * goal(fish)          # seek/arrive toward food, or 0

    # low-pass the steering so heading changes are always gradual
    a = 1 - exp(-dt / tau_steer)
    fish.steer_smoothed = lerp(fish.steer_smoothed, steer, a)

    accel = clamp_len(fish.steer_smoothed, max_force)
    fish.vel += accel * dt
    fish.vel = limit_turn_rate(fish.vel, fish.prev_heading, max_turn_rate * dt)
    fish.vel = clamp_len(fish.vel, fish.speed_cap)   # speed_cap depends on state
    fish.vel *= exp(-drag * dt)                       # water drag, gives coasting
    fish.pos += fish.vel * dt
```

Clamp the turn rate separately from the force. A force clamp alone lets a slow fish pivot almost in place, which reads as twitchy.

### Wander

A random steering force per frame gives twitchy motion with no sustained turns. Reynolds' fix is to steer toward a target that drifts along a circle projected ahead of the fish ([Reynolds 1999](https://www.red3d.com/cwr/papers/1999/gdc99steer.pdf)). Drive the drift with smooth 1D noise instead of per-frame random jitter so it stays frame-rate independent:

```
wander(fish):
    fish.wander_t += dt * wander_rate
    theta = noise1d(fish.seed, fish.wander_t) * PI     # smooth, in [-PI, PI]
    center = fish.pos + heading(fish) * wander_dist
    target = center + from_angle(heading_angle(fish) + theta) * wander_radius
    return seek_velocity(fish, target, cruise_speed) - fish.vel
```

### Containment

Use a signed distance function for the pond shape (ellipse, rounded rectangle, or a kidney shape). Look ahead along the velocity; if the probe point is within `margin` of the edge, steer along the wall tangent, turning the way the fish is already leaning. Steering along the tangent looks like a fish following the bank; steering straight inward looks like a bounce.

```
contain(fish, sdf):
    probe = fish.pos + normalize(fish.vel) * lookahead
    d = sdf(probe)                     # negative inside
    if d < -margin: return 0
    n = sdf_gradient(probe)            # points outward
    t = perp(n) oriented toward current heading
    strength = smoothstep(-margin, 0, d)
    return (t * cruise_speed - fish.vel) * strength - n * strength
```

### Flocking, tuned loose

Koi in a pond drift near each other and sometimes follow one another, but they do not school like sardines. So separation is strong at short range, and alignment and cohesion are weak with longer ranges. Give each fish a personality multiplier (±20%) on speed, wander rate, and cohesion so they do not move in lockstep.

```
separate(fish, others):
    sum = 0
    for o in others within r_sep:
        away = fish.pos - o.pos
        sum += normalize(away) * (1 - |away| / r_sep)   # stronger when closer
    return sum * cruise_speed - (sum != 0 ? fish.vel : 0)

align(fish, others):   average velocity of others within r_align, minus fish.vel
cohere(fish, others):  seek toward average position of others within r_cohere
```

Only count neighbors in a forward cone (about 270°). Fish do not react to a fish directly behind them, and this breaks up symmetric standoffs.

### Burst-and-coast

Koi swim intermittently: a few tail beats, then a glide. A study on koi carp found this gait saves energy compared with steady swimming ([study of koi carp burst-and-coast swimming](https://www.researchgate.net/publication/6273911_Kinematics_hydrodynamics_and_energetic_advantages_of_burst-and-coast_swimming_of_koi_carps_Cyprinus_carpio_koi)), and fish adjust speed by changing the burst-to-coast ratio while keeping the cycle length roughly constant ([arXiv:2002.09176](https://arxiv.org/abs/2002.09176)). That maps directly to code:

```
fish.gait_phase += dt / gait_period            # wraps at 1
bursting = fish.gait_phase < burst_ratio
thrust = bursting ? burst_thrust : 0
fish.vel += heading(fish) * thrust * dt       # drag handles the coast
fish.tail_energy = lerp_toward(bursting ? 1.0 : 0.25, tau = 0.3 s)
```

`burst_ratio` rises when a fish wants to go faster (chasing food) and falls when idle. `tail_energy` scales tail-beat amplitude, so the tail visibly works during the burst and nearly stills during the glide. This one mechanic does more for "alive" than any flocking weight.

### Calm parameters (starting point)

| Parameter | Cruise | Feeding | Notes |
|---|---|---|---|
| speed cap | 0.5 BL/s | 1.2 BL/s | Typical cruise lands near 0.3 to 0.4 |
| max force | 0.3 BL/s² | 0.8 BL/s² | |
| max turn rate | 40°/s | 110°/s | The biggest single calm lever |
| tau_steer | 0.6 s | 0.25 s | Low-pass on steering |
| drag | 0.4 /s | 0.4 /s | Glide half-life about 1.7 s |
| gait_period | 2.5 s | 1.2 s | |
| burst_ratio | 0.25 | 0.6 | |
| wander_dist / radius | 2.0 / 0.8 BL | off | |
| wander_rate | 0.15 /s | | Noise time scale; lower is lazier |
| r_sep, w_sep | 1.2 BL, 1.5 | 0.6 BL, 0.8 | Let them bunch at food |
| r_align, w_align | 3 BL, 0.25 | 0 | |
| r_cohere, w_cohere | 5 BL, 0.15 | 0 | |
| containment lookahead / margin | 2.5 / 1.5 BL | same | |

Hover: every 30 to 90 s, a fish may enter a 3 to 8 s "rest" where its speed cap drops to 0.05 BL/s and only the pectoral fins flutter. Stagger these so no more than one or two fish rest at once.

Tuning order: get one fish wandering and following the wall so it looks good alone, then add separation, then alignment and cohesion last. Most "busy" feeling comes from turn rate and tau_steer being too high, not from speed.

## The body: spine chain and tail sway

### Chain

Argonaut's fish ([repo](https://github.com/argonautcode/animal-proc-anim), [video](https://www.youtube.com/watch?v=qlfh_rv6khY)) uses a chain of joints: the head is placed, and each following joint is pulled to a fixed distance behind the one before, with its angle limited relative to the previous joint (`PI/8` in the original, 12 joints, of which the last 2 form the caudal fin). The body outline is drawn by offsetting each joint left and right by a per-joint width. This is cheap, stable, and needs no physics integration.

```
resolve(chain, head_pos):
    chain.angle[0] = heading_angle(head_pos - chain.joint[0])
    chain.joint[0] = head_pos
    for i in 1..n:
        a = heading_angle(chain.joint[i-1] - chain.joint[i])
        chain.angle[i] = constrain_angle(a, chain.angle[i-1], max_bend)
        chain.joint[i] = chain.joint[i-1] - from_angle(chain.angle[i]) * link_len
```

For a terminal I suggest **8 joints** (6 body, 2 tail), `link_len = BL / 7`, `max_bend = PI/10`. A slightly tighter bend limit than the original keeps turns graceful at low resolution. Widths as fractions of BL, head to tail: `0.10, 0.13, 0.14, 0.13, 0.11, 0.08, 0.05, 0.03`.

Verlet segments (positions plus previous positions with distance constraints) also work and give more floppy secondary motion, but they need several constraint iterations per frame and damping to stop jitter. The follow-the-leader chain gets 95% of the look with a single pass. Use Verlet only for things that should flop, like long butterfly-koi fins, and even then as a 3-point chain hanging off the tail joint.

### Tail sway

The chain alone does not undulate. A fish swimming straight stays straight, which looks like a sliding stick. Add a traveling wave at draw time, perpendicular to each joint's direction, growing toward the tail. Carangiform fish (koi are close enough) use a wavelength of about one body length and a peak-to-peak tail amplitude near 0.2 BL, and swim at a Strouhal number St = f·A/U between 0.2 and 0.4 ([Optimal Strouhal number for swimming animals](https://www.researchgate.net/publication/48199542_Optimal_Strouhal_number_for_swimming_animals), [JEB: Swimming fish stick to same Strouhal number](https://journals.biologists.com/jeb/article/217/13/2224/12210/Swimming-fish-stick-to-same-Strouhal-number), [APS Physics: Teaching fish how to swim](https://physics.aps.org/articles/v10/s91)).

Pick St = 0.3 and solve for frequency from the current speed:

```
A_pp   = 0.2 * BL * fish.tail_energy            # peak-to-peak tail amplitude
U      = max(|fish.vel|, 0.1 BL/s)              # floor so a hovering fish still breathes
f      = clamp(0.3 * U / A_pp_full, 0.3, 2.0)   # Hz; use the full 0.2 BL here
fish.wave_phase += 2*PI * f * dt                # accumulate; never compute f*t

for i in 0..n:
    s = i / (n-1)                               # 0 at head, 1 at tail
    amp = (A_pp / 2) * (0.1 + 0.9 * s*s)        # head barely moves
    lateral = amp * sin(fish.wave_phase - 2*PI * s)   # wavelength ≈ 1 BL
    draw_joint[i] = joint[i] + normal(angle[i]) * lateral
```

At cruise (U = 0.4 BL/s) this gives f = 0.6 Hz, a slow, relaxed beat. At a feeding rush (1.2 BL/s) it rises to 1.8 Hz. Accumulating the phase matters: computing `sin(2π·f·t)` with a changing `f` makes the tail jump.

Also add a small turn bias: when the fish is turning, shift the wave center toward the outside of the turn by `k * turn_rate` so the body curls into turns. The chain already does most of this.

### Fins and head

- Pectoral fins: two ellipses at joint 1, angled back ~45°. Scale their spread with `1 - tail_energy`, so they flare during glides and hovers (fish brake and stabilize with them) and tuck during bursts.
- Dorsal fin: skip at terminal resolution, or draw as a thin lighter stripe down joints 2 to 4.
- Mouth at the surface when eating: briefly widen the head width by 15% and emit a small ripple into the water sim.

## Food: seeking and competition

### Pellets

- A pellet floats and drifts with the water surface velocity from the water simulation, plus light damping.
- It dissolves (fades out) after 60 to 90 s if not eaten. That keeps the pond clean if the user overfeeds.

### Noticing food through ripples

Tie noticing to the water. When a pellet lands, it creates a ripple. A fish notices it when the ripple front reaches it, plus a personal reaction delay:

```
notice_time = t_land + distance(fish, pellet) / ripple_speed + rand(0.3, 1.2) s
```

Fish farther away turn later, so the response spreads outward as a wave instead of every fish snapping at once. Cap noticing at `r_notice = 10 BL`; beyond that, fish only join if they see other fish rushing (use cohesion toward feeding neighbors with a higher weight).

### State machine

```
Cruise   --food noticed & hungry-->  Approach
Approach --within 0.3 BL of pellet--> Eat
Approach --pellet gone--> Linger
Eat      --0.4 s--> Linger (pellet removed, hunger -= 0.15, ripple)
Linger   --new pellet in range--> Approach
Linger   --4..8 s--> Cruise
```

- **Approach** uses arrive: full feeding speed far away, slowing inside `slow_radius = 1.5 BL` so the fish glides onto the pellet instead of overshooting.
- **Linger** is a slow, tight wander (radius 0.5 BL, speed cap 0.3 BL/s) around where food was. Real koi hang around after feeding; this also stops the pond from snapping back to "normal" too fast.

```
arrive_velocity(fish, target):
    to = target - fish.pos
    d = |to|
    speed = feeding_cap * min(1, d / slow_radius)
    return normalize(to) * speed
```

### Choosing which pellet

Each fish scores reachable pellets and picks the lowest:

```
score(p) = distance(fish, p) + 2 BL * (fish targeting p) - 1 BL * (p is current target)
```

The crowding term spreads fish across pellets instead of all converging on one. The current-target bonus is hysteresis so a fish does not dither between two equal pellets. Re-evaluate every 0.5 s, not every frame.

### Competition and hunger

- **Hunger** in [0, 1] rises slowly (0 to 1 over about 5 minutes). Eating subtracts 0.15. A fish with hunger below 0.2 ignores food 70% of the time. This limits frenzies naturally: a big handful gets eaten by the hungriest fish first, and the rest drift in slowly.
- **Size matters a little**: in separation, weight the push by the other fish's size ratio, so small fish yield to big ones. Big koi end up at the center of a feeding cluster, which matches what people see at real ponds.
- **Ties**: if two fish are within eat range of the same pellet, the one whose mouth is closer eats; the other goes to Linger and re-scores.
- **Keep it calm even at feeding time**: feeding speed cap 1.2 BL/s and turn rate 110°/s are still gentle. The excitement should come from many fish converging and the surface ripples, not from any single fish moving fast.

## Depth (optional, cheap, big payoff)

Give each fish a depth `z` in [0, 1] (0 = surface). It wanders slowly with its own noise, rises toward 0 when approaching food, and eases back down after lingering. At draw time, deeper fish are dimmer, bluer, and slightly smaller, and are drawn under surface effects. This gives layering from a top-down view and makes "rising to the food" readable. Separation only counts neighbors whose `|dz| < 0.3`, so fish can pass over each other.

## Turtles: a slower second species

Turtles reuse the brain (same steering code) with different numbers and states, and replace the spine with a rigid body.

### Body

- Rigid shell, oriented along the velocity heading, rotated with a low-passed heading so it turns slowly.
- Four flippers animated with a stroke cycle: front flippers sweep back together, then glide. Rear flippers trail and steer. Head extends slightly during glides and pulls in a little on the stroke.
- Stroke-and-glide mirrors burst-and-coast: `stroke_period = 1.5 to 2.5 s`, stroke takes the first 35%.

### Behavior

| Parameter | Turtle | Koi (for comparison) |
|---|---|---|
| speed cap | 0.2 koi-BL/s | 0.5 |
| max turn rate | 20°/s | 40°/s |
| tau_steer | 1.2 s | 0.6 s |
| wander_rate | 0.08 /s | 0.15 /s |
| flocking | none | loose |

States:

- **Paddle**: slow wander, depth mid to deep.
- **Breathe**: red-eared sliders surface for air every 5 to 7 minutes when active and can stay down 30 minutes or more ([All Turtles](https://www.allturtles.com/how-long-can-red-eared-sliders-hold-their-breath/)). Compress that for the game: every 60 to 180 s, rise to z = 0, hold still 3 to 6 s with the head out, emit one small ripple, then sink.
- **Bask**: if the pond has a rock or log, a turtle sometimes climbs out and sits for 1 to 4 minutes, limbs stretched. Sliders bask in groups and slip back into the water when disturbed ([ReptiFiles](https://reptifiles.com/red-eared-slider-care/red-eared-slider-behavior-handling/)), so a mouse click near a basking turtle can make it slide in.
- **Food**: turtles notice food late (reaction delay 1.5 to 3 s) and approach at their normal slow speed. They sometimes win a pellet the koi missed, which is a nice small moment.

Interaction: koi treat a turtle as an obstacle with separation radius 1.5 turtle lengths and weight 1.2, so fish part around a turtle as it passes. Turtles ignore koi. Limit the pond to one or two turtles.

## Implementation notes for Rust

- Use `glam::Vec2` (f32) for vectors. For smooth 1D noise, a 20-line hand-written value noise or the `noise` crate is enough.
- Struct-of-arrays is not needed at this scale. A `Vec<Koi>` with plain fields is clear and fast.
- Keep the brain and body in the same `Koi` struct, but update brains for all fish first (they read neighbors), then resolve bodies.
- Test the calm numbers headlessly: run the sim for 10 minutes of game time at a fixed seed and assert that max turn rate, max speed, and minimum spacing stay within limits. This catches regressions when tuning weights.

## Sources

- Craig Reynolds, *Steering Behaviors for Autonomous Characters* (GDC 1999): https://www.red3d.com/cwr/papers/1999/gdc99steer.pdf and https://www.red3d.com/cwr/steer/
- Daniel Shiffman, *The Nature of Code*, ch. 5 Autonomous Agents: https://natureofcode.com/autonomous-agents/
- Argonaut, procedural animal animation (source and video): https://github.com/argonautcode/animal-proc-anim, https://www.youtube.com/watch?v=qlfh_rv6khY
- Burst-and-coast swimming of koi carps: https://www.researchgate.net/publication/6273911_Kinematics_hydrodynamics_and_energetic_advantages_of_burst-and-coast_swimming_of_koi_carps_Cyprinus_carpio_koi
- Burst-and-coast swimmers keep a constant cycle length: https://arxiv.org/abs/2002.09176
- Strouhal number in swimming animals: https://www.researchgate.net/publication/48199542_Optimal_Strouhal_number_for_swimming_animals, https://journals.biologists.com/jeb/article/217/13/2224/12210/Swimming-fish-stick-to-same-Strouhal-number, https://physics.aps.org/articles/v10/s91
- Red-eared slider breathing and basking: https://www.allturtles.com/how-long-can-red-eared-sliders-hold-their-breath/, https://reptifiles.com/red-eared-slider-care/red-eared-slider-behavior-handling/
