# ptyrc-apps

Client apps for ptyrc: small programs that draw on a wrapped terminal.

```sh
ptyrc run vim notes.md            # in one terminal
ptyrc-app confetti                # from another
ptyrc-app clock --prefer bottom-right --big
python -m ptyrc_apps.clock -s NAME
```

| app | what it shows |
|---|---|
| `confetti` | the pops of the first ptyrc: finite animations of 3 by 3 cells, four colours a burst, behind the text; `--bursts N` at once at random offsets, each shot on a cold spot of the heatmap; a round every 3 s until interrupted; `--one-shot`: one round of persistent sprites: the app exits, the driver plays them. `--fancy`: big explosions, a native effect started with one call; `--random`: pops of several kinds, explosions of random size, scale, shape and colours; `--fireworks`: a rocket first, one sprite with a `motion` that brakes; `--sparkle`: twinkles; `-a`: all |
| `clock` | one widget, a variant per face: `HH:MM` up to block digits with the date and week, a braille dial; the driver picks the face and the place, the deck draws every face each second (`Deck.run(every=True)`): one the driver puts is never late; `--tz`, `--utc`, `--no-seconds`, `--big` (block digits only), `--print` (every face, no driver) |
| `monitor` | the state of this machine in widgets the driver places: a tiny glances (CPU, cores, memory, swap, load, network, top processes), who is logged in (grouped, cleaner than `w`), TCP ports open to the outside, free space of the local disks; each a tag of variants from one line to a small table, `render(data, w, h)` pure; `--monitor LIST`, `--merged`: one widget showing them in turn; `--print [WxH]`: no driver, the variants in boxes on stdout |
| `git` | a repository in widgets the driver places, read and never written (`_gitprobe`: `GIT_OPTIONAL_LOCKS=0`, no command but a read, not `git diff`, which refreshes the index): `--repo`, else the one the wrapped program is in (`/proc/<pid>/cwd`, followed when it moves), else the app's; read every `--every` seconds (5) and within a second of a change of the git directory (`_gitprobe.Watch`: mtimes). Bare: `head`, `main ↑2 ↓1 ●3 +1 ?4` up to branch, upstream, an operation left open, the counts, the last commit. `-t`: `status`, as `git status -s` with the lines added and deleted; `-l`: `log`; `-g`: the log as a graph, a colour a lane, with the refs; `-b`: `branches`; `-S`: `stash`; `-w`: `worktrees` (both hidden while there is none); `-p`: `pulse`, commits a day in braille; `-W`: the border of `head` chases while something wants a look (conflicts, an operation open for 15 min, upstream gone, behind, a tree changed for `--dirty` hours); `-a`: all. The host mouse over a commit of `log` tells it in full (`Deck.on_hover`). `--monitor LIST`, `--merged`, `--print [WxH]` as `monitor` |
| `note` | a file as a sticky note, read again when its mtime changes, written with `-i` only: `TODO.md` by default, a widget a file; markdown reduced to boxes (`- [ ]`, `- [x]`), bullets and headings; variants from `3/7` to the note wrapped: too long, it goes without what is done, then to the open boxes alone, then is cut with `…`. `-c`: the boxes ticked, in the title and as a bar; `-x`: what is done dimmed; `-o`: what is done left out; `-p`: a yellow ground and a border of blocks with a folded corner, given as a border object (`note.POSTIT`); `-a`: `-c -x -p`; `--merged`: one widget, the notes in turn; `-i`, not in `-a`: the notes are quiet interactive widgets (`Deck.interact`): ctrl+t twice focuses one, then a list, an item a row, with a bar and a foot of keys (`note.Desk`, pure): `space` or `enter` ticks a box, `a` adds one under the bar, `e` edits the words as written (a `ui.Editor`), `enter` ends, an empty line cancels, `d` then `y` deletes, `shift+↑↓` or `K` `J` moves the item; esc leaves, what was being typed is dropped; ctrl+x closes the note, the app ends with the last; a change is one of a line (`note.change`), written at once by a rename (`note.edit`), the other lines kept as they are; a file that changed since it was read is read again and the key lost; with `--merged` the turn stands still while focused; `--print [WxH]` |
| `ask` | a question from a shell, on `ptyrc.ui`: `ask QUESTION OPTION...` a choice (`-m`: several, one a line; `-o`: a last option, typed), no option: a line, `-t`: several lines; `--default` (the text, or the option under the bar), `--title`, `--timeout SECONDS`, `--json`. The widget waits, loud: ctrl+t twice focuses it, `enter` validates, ctrl+x dismisses, esc leaves it there. The answer on stdout, status 0; dismissed, timed out or the session ended: status 1, nothing; no session: 3 |
| `timer` | one widget, a face a variant, `face(state, w, h, look)` pure: `MM:SS`, the time and a bar that empties, block digits (those of `clock`); `timer 25m` (`90s`, `1h30m`, `1:30`, `25`: minutes), `--until HH:MM`, `--pomodoro 25/5 --rounds 4` (0: no end); the state is that of a `Run` on the session clock, `--speed N`; the app draws when a second ends, not on a period. `-g`: an hourglass, 5x6 to 13x14, alone (the time is the title of the widget) or next to block digits: walls of glyphs, the sand in braille and counted: `sand(k, n)` is the glass after `n` grains fell, one every `length / N` seconds, the last as the time ends; it leaves the upper bulb by the middle and heaps under a stream; a new phase turns the glass over, all in braille; under the host mouse (`Deck.on_hover`), the time in digits over the neck; `-c`: colours by the share left, the break in cyan; `-r`: the border `alert` at the end, and 6 s at a new phase; `-b`: a burst of `confetti` behind the text; `-a`: all; `--modal`, not in `-a`: at the end, shown again as a modal; `--click`, not in `-a`: a click on the widget pauses (topic `mouse`); `--hold SECONDS`: leaves after the end, else stays, `done +1:15`; `--print [WxH]`, `--at SHARE`: no driver, the faces at a moment |
| `pace` | keys a minute of the host keyboard: topic `input`, the bursts counted (`keys`: an escape sequence is one key, a paste none) and dropped; the pace of the last 15 s (`--window`). `-g`: half an hour in braille; `-b`: the highest pace; `-w`: the words typed most, read from the screen, never from the keys: after a key the row of the cursor is read (`cursor`, `text`), a `Reader` follows the word that grows under the cursor a cell a key and takes it when a separator or a new line ends it; what is not echoed (a password) is never read, what a program writes or a paste brings is not typed; left out: words under `--min` (4) letters, a stop list (`--stop FILE` for more), and what `wordy` refuses: hashes, keys, base64, what does not look like a word; `-r`: a bar a word; `-c`: colours, a rank each; `-a`: all; under the host mouse, a word's count and when it was last typed; counts in memory, `--save FILE` keeps them (mode 600); `--top N`; `--print [WxH]`: no driver, a made-up session (`--seed`) |
| `minimap` | the text of a frame in braille, a widget: the rows the driver remembers of it (`frame.text`: the island on screen), a dot `cols` columns by `rows` lines, on when half of its lines hold ink there, the least scale that fits; the view a bright band; a column `--width` cells across (12), no taller than its text: the heights offered go up to the one that shows a line a dot row, the tag declared again as the text grows, the driver picks among them; a round border (`--border`), a cell from the edge of the view as any widget, the lines known on its bottom edge. Read again on the topics `text` and `frames`, never on a period. `-c`: the line of the cursor; `-H`: a hue for the lines edited, another for those visited, fading in `--fade` s (60); `-n`: comments in green; `-a`: all; `--mark PATTERN`: the lines of a regular expression in a colour, one a pattern; under the host mouse, the line there, told beside the map; `--print [WxH]`: no driver, the map of `--file` |
| `flower` | flowers of random size and colours planted on cold cells next to the matches of a word (`--count N`, 0: all), the word bolded with a tint; `-a`: anywhere, next to `--count` words drawn from the screen (default 5; 0: 10 to 30, redrawn each minute), one renewed every 10 s, no bold; grown by the client, swayed by the driver; frame coordinates |
| `fishes` | sprites in frame coordinates: they scroll with the text; one scrolled out of the view goes after a few seconds and comes back behind the text; half of them shy, hiding behind the text; the cursor and the text told by the driver (`_cli.Sight`), not asked; one batch per tick. `-d`: decorations standing on the text, behind or in front of the fishes; `-b`: bubbles, each one request with a `motion`; `-r`: random species, events (a creature crossing, an object sinking, a hook); `-a`: all. Populated areas are remembered in frame cells (`_reef.Reef`). `--tank`: the fishes in one widget (`fishes.Tank`), from a bowl of 9x4 with one fish to a tank of 40x10, border `tank` given as an object; composed in the content, a `widget.set` a tick: a sprite is clipped to its frame, not to a widget; a row of surface; there `-d`: sand and decorations on it, some in front of the fishes; `-b`: bubbles that grow as they rise; `-r`: random species; hover: the fishes come to the pointer; `--print [WxH]` |
| `snake` | the screen read as a map: glyphs are walls, read again when the topic `text` says so (`_cli.Sight`); one sprite redrawn in place each step. `-e`: particles, each one request with a `motion`, golden apples, a shaded body, the score; `-x`: foes that bite the tail off; `-l`: beams from wall to wall that burn what they cross; `-a`: all (`_arcade`); `--front`, not in `-a`: over the text; `--grab`, not in `-a`: the arrows of the host keyboard steer the snake, at half the speed; against a wall it waits, into its body it bites itself off; ctrl+t gives them back and ends |
| `bricks` | a brick breaker that plays alone on the screen read as a map: the ball bounces off the glyphs, the sides of the view and the bricks; the bottom is open, and the top with two rackets. The ball is half a cell high, `▀` or `▄`. One racket, `(━━━)`, moved in two dimensions and hit on either side: it foresees the ball, gets in its way before it leaves, and takes the shot that breaks the most bricks; idle, it hits a safe ball on purpose when that breaks more. Text does not stop a racket (under it, it is hidden, and the ball meets the text); it keeps off the text when it can. Bricks of 3 cells, `⟦=⟧`, in frame cells, laid a group at a time all over the view, far from each other, where a ball gets, near the top of the view when it has room there (near the middle with `-m`): large groups, a colour a row: medium walls of 2 rows by 3 to 5 bricks (a row or a pile where the place is tight), now and then a big wall of 3 or 4 rows by 6 to 8; where no large one fits, groups of 2 or 3; a group goes 50 to 100 s after it is laid, two seconds later for each brick over three, dim for its last second. The last ball out: the score at 0. 30 s with no brick broken: the game starts again on a new map, the scores stay. `-r`: hard `⟦≡⟧`, loose `⟨=⟩`, bumper `⟦o⟧` and bomb `⟦*⟧` bricks; `-b`: capsules, two broken bricks in five: two balls for one (three times in five), wide, twin, fire, small, fast; `-m`: two rackets, two scores, bricks first; face to face, one keeps away for 2 to 3 s; `-e`: particles, a trail, combos, a best score; `-a`: all (`_bricks`); `--grab`, not in `-a`: the arrows of the host keyboard move the racket (with `-m`, the bottom one; the other plays alone), the balls go half as fast, with `-m` the other rackets move as fast as the player did over the last 4 s, ctrl+t gives them back and ends |
| `life` | Conway's game of life in braille: a cell of the terminal holds 2 by 4 cells of the world, a torus as large as the pane; one sprite in view cells, behind the text, replaced each generation (`--speed`, 10 a second); nothing drawn in the cell on each side of a glyph: the words stay apart; a world gone still is made again. `-c`: a glyph coloured by the age of its youngest cell; `-t`: the cells that died fade out; `-g`: patterns of a library of 240 (`_patterns.py`: still lifes, oscillators, spaceships, methuselahs, guns, puffers) dropped in the holes of the world, the widest first, a large pattern in a wide hole, a ship turned to the open; the world starts with them, no soup; `--rate`: patterns a second into an empty world, 4 times fewer into a crowded one (2; 0: only when it is still); `--kinds ship,gun`: the kinds dropped (`all` adds `filler`, which grows over the whole world); `-p`: particles where cells die together and where a pattern lands; `-w`: no cell lives under the text; `-a`: all; `--front`, not in `-a`: over the text |
| `stars` | a night sky in view cells, behind the text: a map of stars laid over the pane, the same at every size (`--stars N`, `--seed`); a moon in braille, tonight's phase (`--phase`), only its lit side drawn, that crosses the sky on an arc (`--night SECONDS`, 300; `--moon DOTS`); grass in braille along the bottom, each blade leant by a breeze (`--wind`): a wave, gusts, a flutter; taller in a taller pane, twice on 50 rows what it is on 15; the host mouse in it parts the blades, which swing back when it leaves; the scene a function of the time and of where the mouse has been, two sprites, sent when they change; nothing drawn in the cell on each side of a glyph. `-t`: the stars twinkle; `-r`: the sky turns; `-c`: three constellations, their lines drawn; `-g`: the milky way; `-m`: a shooting star now and then, one request with a `motion`; `-b`: fireflies over the grass; `-w`: a meadow, taller grass and flowers; `-a`: all; `--no-grass`: the sky only, no grass, `-b` nor `-w`; `--print [WxH]`: no driver, the scene on stdout |
| `pond` | still water in view cells, behind the text, seen from above, all braille: a dot is square, a ring round; few things, dull colours; a drop now and then (`--drops N` a minute), drawn from the second it falls in, so the scene is a function of the time; its rings leave fast, slow down, lose arcs and thin as they fade; lily pads, a map laid over the pane, the same at every size (`--pads N`, `--seed`); light on the swell, adrift on a slow current (`--current SPEED`, dots a second); the words are stones: a drop on one makes no ring, a ring stops against a line and closes again behind a word (`pond.shadow`: per bearing, the distances hidden); one sprite, 20 times a second, sent when it changed; nothing drawn in the cell on each side of a glyph. `-k`: koi under the surface, that steer clear of the words and send a ring now and then; `-l`: lotus in bloom on the pads; `-r`: showers that come and go; `-m`: the moon in the water, tonight's phase (`--phase`), shaken by the swell, that crosses the pond (`--night SECONDS`, 300); `-p`: what the season of the day sheds, adrift on a current: petals, leaves (`--season`); `-i`: water striders, a ring at each push, and dragonflies, sprites the driver plays; `-a`: all; hover: the pointer is feared while it moves and 3 s after: the koi dart away, the striders slide off, a dragonfly leaves; `--print [WxH]`: no driver, the scene on stdout |
| `shore` | a beach in view cells, behind the text, seen from above, all braille: sand at the top, in windrows, a map laid over the pane (`--sand N`, 420; `--seed`); the sea at the bottom. A wave every `--waves SECONDS` (8): a crest, a line of long strokes, that comes in, breaks, runs up the sand in lobes behind a front of foam, a net carried by it, and slides back; it leaves a line of foam at its highest reach, which thins over the next waves and is wiped by the first to pass it, and wet sand. The tide moves the still-water line up and down the pane (`--tide SECONDS`, 300). The scene is a function of the time; few marks, the sand and the water left almost bare. The words are rocks: froth gathers over and under them in the water; nothing is drawn beside a glyph. `-d`: shells, pebbles, a starfish, a line of wrack; `-r`: ripple marks in the sand the tide uncovers; `-c`: crabs, sideways, under the sand when a wave comes over; `-b`: sandpipers, in groups, that follow the water down and flee it, well off the foam; a group walks off the side of the pane and comes back, or wanders up the dry sand, and so does a bird alone; `-w`: the footprints of someone walking by, wiped by the waves; the host pointer over the pane scares the animals: sandpipers run from it, and fly, all those about, when it is on one; crabs run for their lives; a pointer that rests 5 s is forgotten, they walk back; `-n`: dusk, night and dawn (`--day SECONDS`, 240): the foam glows, the moon glitters on the water; `-a`: all. `--at SECONDS`; `--print [WxH]`: no driver, the scene on stdout |
| `flock` | starlings before a sunset, in view cells, behind the text, all braille; the sky and the land a function of the time, the birds stepped 10 times a second and drawn 30 times, on a smooth curve through their steps; three sprites sent when they change: the sky, the sun and the birds, the land. A dusk (`--dusk SECONDS`, 300; `--at F`): the sun (`--sun DOTS`) comes down from over the view, yellow to red, sets behind a line of hills; the sky glows round it and along the horizon, strokes of light that grow with it, gold to purple; a night, and the dusk again. The flock (`--birds N`, 100; `--seed`): a bird a dot, each on its own wings: it flies with its neighbours, after a lead that wanders; a few stray, now and then a part goes its own way and comes back; they flee the host pointer; a cell the brighter the more birds; a bird before the sun is a dot off in its disc; at nightfall they glide down, part after part, to roosts all along the land, and take off from there at the next dusk; nothing drawn in the cell on each side of a glyph. `-c`: bars of cloud that drift, lit from under; `-w`: a lake, ripples, the sun's path of light; `-r`: reeds in a breeze (`--wind`); `-j`: groups come in from the sides and join; `-d`: a second, smaller flock; `-k`: a hawk hunts now and then: it stoops at the thick of the flock, which bursts apart and gathers again; `-n`: the stars come out; `-o`: the birds roost on the lines of text; `-a`: all; `--print [WxH]`: no driver, the scene on stdout |
| `window` | a house window on a landscape, one widget, a variant a size from a porthole of 7 by 3 to a bay of 40 by 12; the border is the frame, a sill under it, the mullions are content; pixels of half a cell (`_canvas.Half`), colours of the 256; the scene a function of the time: the light, the sun, the moon in its phase and the stars of `_sky`, a land of `_strip` from a seed, one a day (`--seed`); bare: the sky, a hill, a meadow, drawn again every 20 s. `-l`: ridges, trees, houses lit at night; `-y`: the season of the date: blossom, green, leaves turned, snow; `-w`: the weather of the day: clouds, rain, fog, snow (`--weather`); `-p`: birds, smoke, a balloon, a meteor, a plane, the stars twinkle; `-c`: curtains, a cat on the sill; `-a`: all, and the place of the day (`--place`: countryside, sea, mountain, city); the cat (`_mouser.py`) alone sits, looks aside, grooms, stretches, watches the birds, sleeps at night; the host mouse over it (`Deck.on_hover`): the curtains move aside, a house lights up, the cat hunts the pointer all along the sill: it runs, crouches, pounces, bats what it caught, stands up and leaps under what is out of reach, looks for what it lost, walks home; 12 frames a second meanwhile; `-r`: what is not given at random, another at each run: the place, the season, the time (a low sun or an evening more often than a noon), the weather (`--seed`: the same draws); `--at HH:MM`, `--season`, `--speed N` (1440: a day a minute), `--lat`, `--rgb`; `--print [WxH]`: no driver, the variants on stdout |
| `train` | the land going by the window of a train car, one widget, variants from 10 by 3 to 48 by 11, a rounded frame with a rail under the glass; layers of `_strip`, the land of each line in `_line`, each at its depth: the far ones barely move, the near ones run; the whole content sent 24 times a second, the nearest plane a pixel a frame, each layer a pixel every so many frames; far to near, nothing shows through what is nearer; bare: the sky, two ranges of hills, the verge. `-l`: far ridges, fields, trees, houses, the ballast, what brushes the glass; `-p`: poles on the verge and their sagging wires; `-e`: a tunnel (a cutting, a portal, a wall of bricks and its lamps), a truss over a river, a train the other way, a level crossing, a station where it brakes and stands 10 s, one a 70 s; `-y`: the season; `-w`: the weather, rain along the glass; `-a`: all, and the line of the day (`--line`: plain, coast, alps, desert, city); the host mouse on the glass: a breath of fog, gone in 6 s; `-r`: what is not given at random, another at each run: the line, the season, the time, the weather, where along the way (`--seed`: the same draws); `--after SECONDS`, `--at`, `--season`, `--speed`, `--lat`, `--rgb`, `--print [WxH]` |
| `fire` | one widget, from a candle (3x3) to a hearth (45x13): a field of heat, a function of the place and the time (`fire.scene`, pure: tongues that lean and lick, turbulence that rises), drawn ten times a second; bare, glyphs ` .:^*x#`, red to yellow. `-b`: half blocks, two rows of heat a cell; `-l`: what burns: a candle on its dish, a brazier, logs on andirons, and the border `hearth`, given as an object; in blocks with `-b`, else in glyphs, `(@)=-~==)`, a pile of them in the wide hearths; `-c`: it burns down to embers over `--burn` seconds (3600), the logs turn to coals, then a log is put on; `-k`: sparks rise and die; `-g`: the light: a halo, with `-l` the bricks behind the logs, lit near the flames; `-a`: all; hover: a poke, the flames rise and sparks fly; `--speed`, `--seed`, `--print [WxH]` (no driver, `--at SECONDS`) |
| `globe` | a snow globe in one widget without a border, the dome is the content, four sizes from 11x5 to 30x13: a house between two trees, a little snow; `globe.scene` pure: a flake is a function of the time and of the last shake. `-k`: shaken now and then (`--every`, 45 s): the snow swirls, falls thick, thins; `-d`: the snow lies: drifts on the ground, the roof and the trees turn white, a shake lifts it; `-l`: the window flickers, a garland on the trees, a glint slides along the glass; `-r`: a scene at random (house, snowman, church, fir), another at each shake, `--scene NAME`; `-a`: all; hover: a shake; `--speed`, `--seed`, `--print [WxH]` |
| `gallery` | framed pictures in a widget, the `painting` border, the name on the plate: one hung at a time, another every `--every` (10m), drawn in a shuffled order (`--seed`); the sizes of a picture are the variants of its tag, declared again at each picture (`Deck.resize`); a picture is a text file, `pictures/*.pic`: glyphs, an ink a cell (colours of the 256, a ground too), frames, a period (`gallery.parse`); `--kind LIST` (portrait, landscape, still, animal, abstract, silly), `--file`, `--dir`: the owner's own; `pictures/SHIPPED`, when there, lists the ones hung (`--candidates`: all). `-m`: the pictures move, two to six slow frames; `-l`: the plate names the author; `-o`: under the pointer the eyes of a portrait follow it and the plate names the author (`Deck.on_hover`); `-w`: three at once, changed in turn (`--count N`); `-a`: all; `--speed N`; `--print [WxH]`: no driver, the pictures on stdout, `--colour` |
| `sand` | `Pilot.wait_idle` to choose a pane; a native simulation from a draw extension that reads the pane's cells, its walls, by itself; the pile is fixed on frame rows and scrolls with the text; `--cursor` pours on the cursor's column; `--hover`: the host mouse takes the sand it is over, the pile falls in; `--bomb`: a click on the pile blows it up, the sand flies and lands, the longer the press the stronger (1.5 s at most), towards the cell where it is released; the clicks on the pane are then the app's; `-a`: `--cursor`, `--bomb`, `--now` |
| `cat` | an app driven by events (`idle`, `cursor`, `screen`, `frames`), asleep on the socket between two things to do; it stands on lines of text in frame coordinates (`cat.ledges`), sleeps when the pane is idle, and jumps away from the cursor, from text written over it (`covered`) and back into the view after a scroll (`visible`). `--count N`: cats that share one read of the view, its shape with the words `food`, `fish`, `toy` (`cat.Home`) and keep off each other, a coat each; `-r`: coats and features (ears, eyes, nose) drawn. The word `food` on screen gets a plate next to it, where the cats come and eat; `--food`: a plate on a ledge now and then; `--mice`: a mouse walks a ledge now and then, the cats pounce at it. The word `fish` gets a fish (`--fish`: one now and then): a cat takes it, carries it away and eats it there, where another may steal it. The word `toy` gets a ball of yarn (`--toys`): a cat bats it: it jumps (a hop, or a leap over 3 rows of text), bounces, rolls, falls off the end of its line; the cat runs after it; its thread, braille, sinks to the floor along its way and hangs from the lines; it goes once played out. `--hunt`: the cats hunt the host mouse (topic `hover`, told over a sprite of nothing as large as the view): they run, wiggle, pounce, bat it, beg under it, purr when it rests on them. `-a`: all of these, two cats unless `--count` |
| `marks` | a highlighter: one tint per match of each pattern (`-e` regex, `-i`), in frame coordinates and on remembered lines too (`find(known=True)`), so the driver scrolls them with the text; searches of a look in one batch; topic `draw`; a display of the counts placed by the driver |
| `rain` | a native effect from a draw extension that sees the text: drops fall behind it and splash on the top of its lines, on the cursor and on the floor; less rain on columns that changed in the last 10 s (the heat, given to the effect) |
| `snow` | a native effect that sees cells and heat: flakes fall behind the text; with `--cover`, snow lies over the lines of text (`--max-depth` rows deep, 3; 0: no limit, the pane fills), a cell away from the words of its own row, and on the floor, in smooth piles up to three rows deep; it melts where the screen changed lately and around the cursor; `--hover`: the host mouse takes the snow it is over, the snow above falls in, the piles beside slide in; `-a`: `--cover`, `--hover`, no limit; it lies on frame rows, so it scrolls with the text; the wind is tuned in place (`Sprite.tune(wind=...)`), the effect keeps its state |

`ptyrc manage` starts, stops, pauses and restarts them on a session; in
a `ptyrc run`, ctrl+t typed twice opens its menu. `ptyrc-app --catalog`
prints the apps and their options as JSON (`_catalog.py`, from the
argparse parser of each app): what the menu shows. `random`, there, is
what `random app` draws from (`marks`, `note` and `ask` are never
drawn).

An app that can change frame while it runs subscribes to the topic
`move` (`_cli.movable`); `ptyrc manage move` then sends it `{"frame":
id}` in place of restarting it. `_cli.follow` takes it (`sand`, `rain`,
`snow`: the sprite goes to that frame), and `_widgets.Deck` (`clock`,
`monitor`: the tags are shown again there). On a driver without the
topic the subscription is refused and the app runs as before.

`scripts/demo-apps.sh` runs them on a scrolling vim buffer;
`scripts/demo-frames.sh` gives each pane of a tmux session its own
drawing. Both take `--check`.

## Writing an app

An app is a module with `main(argv) -> int`, listed in `APPS`
(`src/ptyrc_apps/__init__.py`). `_cli` gives it the common options and a
connection:

```python
from ptyrc import Pilot
from . import _cli


def app(pilot: Pilot, args) -> None:
    note = pilot.draw(["hello"], frame=args.frame, auto="top-right", fg="yellow")
    for nth, _ in enumerate(pilot.find("TODO")):
        pilot.draw(["<-"], near="TODO", nth=nth, fg="red")
    pilot.wait_for("done", timeout=60)


def main(argv=None) -> int:
    out = _cli.parser("hello", "Say hello.")
    return _cli.run(app, _cli.parse(out, argv))
```

What to know:

- **Sprites die with the connection.** Return, crash or ^C: the driver
  removes what the app drew. Pass `persist=True` to leave something behind.
- **Where**: `at=(x, y)` with `space="view"` (fixed on the pane; negative
  from the right and bottom) or `space="frame"` (moves when the content
  scrolls); `near="word"` (follows the word); `auto="top-right"` (a free,
  still spot, chosen and kept by the driver); `cursor=(dx, dy)`.
- **On what**: `frame=` an id or `raw`, `active`, `largest`, `quietest`,
  `idle`. `pilot.panes()` lists the panes found on screen; a pane is
  drawable once it is `usable`: a new layout settles for 1.5 s first.
  `_cli.pick` falls back to the raw frame where there is no pane.
- **When the frame leaves**: a pane out of sight (another tmux window)
  keeps its sprites; drawing on it answers `not-usable`. A loop starts
  each step with `_cli.on_screen(frame)` and wraps it in
  `_cli.in_sight()`; or with `sight.look()` and `sight.drawing()`.
- **Idle**: `frame.is_idle` is true once 80 % of a pane has been still
  for 10 s. A big animation waits for it (`pilot.wait_idle`, `sand.py`);
  topic `idle` says when it comes and goes (`cat.py`).
- **What became of a sprite**: `Sprite.info()` has `visible` (some of
  it is in the view) and `covered` (share of a back sprite under the
  app's text); topic `draw` lists the sprites removed with rows the
  driver forgot.
- **Events, not ticks**: an app with nothing to animate waits on
  `pilot.events(..., timeout=...)` or `pilot.event(timeout)`: it wakes
  on a change, or when its next step is due (`cat.py`, `marks.py`).
  Silence ends the wait; a session that ended raises `PtyrcError`
  (`gone`), which `_cli.run` takes for a normal end: no loop tests the
  connection.
- **The state at start**: `pilot.subscribe(..., snapshot=True)` sends
  what is (idle, cursor, frames, size) as the first events (`cat.py`).
- **Topic `text`**: per frame, the frame rows whose text changed, and
  the origin after a scroll; a row that scrolls into view is not a
  change. Topic `cursor` tells the cursor in frame cells (`frame`,
  `pos`).
- **A game on a tick, a map on events**: `_cli.Sight(pilot, frame)`
  subscribes to `frames`, `text` and, when asked, `cursor`. Each tick
  starts with `sight.look()`: it takes the events waiting and reads
  the view again only when told (`sight.changed`, `sight.rows`,
  `sight.cursor`): its shape (`Frame.shape`, `words=` shown), or with
  `text=True` its text. What each app reads: `_catalog.READS`. A quiet screen costs no read
  (`snake.py`, `bricks.py`, `fishes.py`, `flower.py`, `life.py`,
  `stars.py`, `pond.py`).
- **A frame replaced**: a pane keeps its frame through a resize, a
  nearly empty one too when it is the only such pane. Otherwise it gets
  a new frame, and the sprites stay with the old one.
  `_cli.follow` waits on the topic `frames`, `_cli.heir` finds the new
  pane in the event's data, in proportion of the resize;
  `Sprite.update(frame=...)` moves the sprite, state kept. Its `beats`
  are the app's own work between two events (`snow.py`).
- **An effect tuned**: `Sprite.tune(wind=2)` gives a running effect
  new parameters; it keeps its state (`snow.py`).
- **An effect told the pointer**: `_cli.follow(..., hover=True)`
  (`_cli.Nozzle`): a hover is told on a sprite, not on an effect, so a
  sprite of blanks lies under the effect; each move is a
  `Sprite.tune(hover=[x, y])` (`snow.py`, `sand.py`). With
  `bomb=True`, the clicks: a press on that sprite and its release
  (topic `mouse`) are a `Sprite.tune(bomb=[x, y, ms])`, or
  `bomb=[x, y, ms, dx, dy]` when released `dx`, `dy` cells away
  (`sand.py`).
- **A score**: `_arcade.Board`, one sprite in view cells with opaque
  blanks, sent when it changes.
- **What**: `rows` (a space is transparent), `cells` (styled runs),
  `frames` (an animation the driver plays alone), `effect` (a native
  generator), `pilot.ext(...)` (a draw extension).
- **Widgets**: a small window the driver places, sizes and moves.
  `pilot.widget(tag, [(8, 1), (20, 5)], content=render)` declares the
  variants of a tag, the same information for boxes of those inner
  sizes, and shows one: where there is room, about half of it, on
  the side `prefer` names (centred by default), in a border with the
  title; a smaller one, or elsewhere, when the app's text or another
  widget needs the place. Behind the text by default.
  Topic `widgets` tells the variant shown; `Widget.set` fills it.
  `_widgets.Deck` does both for an app: `deck.add(tag, sizes, render)`,
  `deck.run(period)` (`clock.py`, `monitor.py`).
  `deck.on_hover(widget, move, leave)`: `move(x, y)` when the host
  mouse, no button down, is on a cell of the inner box; `leave()` when
  it goes, or after `deck.hover_timeout` (1.5 s) without a move; the
  widget is drawn again after each, unless the handler returns False.
  The driver asks the host for the moves (1003) only while a deck has
  a handler; they never reach an app that did not ask.
  `deck.interact(widget, key, quiet=True, escape=None, close=None,
  paste=None, closed=None)`: the widget asks for the keyboard
  (`Widget.ask`); once the user focused it (ctrl+t twice), `key(name)`
  for each key, `paste(text)` for a bracketed paste; the widget is
  drawn again after each, unless the handler returns False, and when
  the focus comes or goes: its `render` reads `widget.focused`
  (`note.py`). Its close key (ctrl+x) hides it: it leaves the deck,
  `closed()` is called; `deck.run` ends with the last. `_widgets.Keys` turns
  the `input` events into keys and pastes, and names the arrows with
  shift. A question that ends is a `ptyrc.ui` model in a `ui.Dialog`
  (`ask.py`). `border=`: `plain`,
  `round`, `heavy`, `double`, `ascii`, `none`, `painting`, `baroque`,
  `mat`, `sketch`, or an object of glyphs (`fire.BORDERS`,
  `fishes.BORDERS`, `note.POSTIT`).
- **A loop of its own**: an app that draws only when something changed
  reads the events itself and gives them to its deck: `deck.handle`
  (all of them), or `deck.take` (`widgets`), `deck.move` (`move`),
  `deck.hover` (`hover`), `deck.input` (`input`), then
  `Widget.set` when the rows change (`timer.py`, `pace.py`).
- **A picture**: `_canvas.Half(w, h)`: `w` by `2h` pixels of colour,
  two a cell (`▀`), glyphs and braille over them; `rows()` is the
  content of a widget or a sprite. `_sky.at(when, lat)`: the light and
  the colours of a time; `_strip`: a land in layers from a seed, the
  sky, the weather, `Pointer` for a hover (`window.py`, `train.py`).
- **In which layer**: the app's text is layer 0. `layer="back"` (the
  default) draws behind it, in the cells it leaves free; `layer="front"`
  over it; integers order more layers (`-2` is behind `-1`), `z` orders
  inside one. A `bg` behind the text shows under it: a sandwich.
- **How it mixes**: `blend="tint"` keeps the app's glyphs and applies
  your colours.
- **Motion from the client**: change a sprite with `Sprite.set` or
  `Sprite.move`; group the changes of one step in `with pilot.batch():`,
  they show in one repaint.
- **Reading**: `pilot.text()`, `pilot.shape(words=)` (`#` where there is
  ink, the words named as shown), `pilot.find()`, `Frame.known()` (what a pane
  remembers outside its view), `pilot.cursor(trail=True)`,
  `pilot.events("screen", "text", "cursor", "frames")`. Inside
  `batch()`, `text`, `shape`, `find`, `sprites` and `Sprite.info` return a list
  or a dict filled when the batch is sent (`marks.py`).

`_cli.ticks(period)` paces a loop and yields the measured step;
`_cli.hold(pilot)` keeps an app with nothing to send connected;
`_cli.follow` does, and follows a frame replaced.

Read `confetti.py`, `clock.py` and `rain.py` first (`monitor.py`
and `git.py` for widgets; `window.py`, `train.py`, `fire.py`,
`globe.py`, `gallery.py` for a scene in one), then `marks.py`,
`cat.py`, `flower.py`, `fishes.py`, `snake.py`, `life.py`,
`stars.py`, `pond.py`, `bricks.py`, `sand.py`.

### A picture for the gallery

No code: a text file in `src/ptyrc_apps/pictures/`, `NAME.pic`. A
header (`name`, `author`, `kind`, `ink`, and for one that moves
`period`, `hold`), then a block a size: `== WxH`, the rows of glyphs,
`::`, the rows of ink keys; `++` starts the next frame. The format is
told at the top of `gallery.py`; `banana.pic` is the smallest, `owl.pic`
blinks and looks at the pointer, `swell.pic` has two sizes.

    ptyrc-app gallery --file NAME.pic --print --colour -m

prints it, every size and frame, or says which line is wrong. Glyphs
one cell wide; at most 40 by 14 and six frames (`test_gallery.py`).
Original drawings only, under the licence of the project.

## Tests

`uv run pytest plugins/client-apps/python`: each app against a headless
driver. An app runs until ^C or the end of its session; a test stops it
with `main(argv, seconds=...)`, or with a stand-in that raises
`KeyboardInterrupt` (`confetti`).
