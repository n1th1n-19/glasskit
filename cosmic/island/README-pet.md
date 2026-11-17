# Island idle pet

When nothing else is showing, the Island shows a small pixel pet that walks, stands, hops and
falls asleep after about a minute idle (it only animates while idle). Click the island while
nothing is playing and the popup is a small glass card with just the pet, bigger, walking across
it; click the pet to make it hop. While music plays the popup is the normal media card.

## Settings: `~/.config/glasskit/island.conf`

`cosmic/install.sh` copies the commented template `island.conf.example` there on first install
and never overwrites an existing file. Format: `key = value`, `#` comments, blank lines ok.
An unknown key or a bad value is ignored (the default is used, one line goes to stderr).
Restart the applet (re-add it in the panel) to apply a change.

| key | values | default |
|---|---|---|
| `pet` | see below | `random` |
| `popup_pet` | `true` / `false`: pet in the empty popup card (`false` = "Nothing playing" + Prev/Play/Next) | `true` |
| `pet_in_panel` | `true` / `false`: idle pet in the pill (`false` = accent dot) | `true` |
| `sleep_after_seconds` | `0`-`86400`; `0` = never sleeps | `60` |
| `walk_speed` | `slow` / `normal` / `fast` | `normal` |
| `hop_frequency` | `never` / `rare` / `normal` / `often` | `normal` |
| `scale` | `auto` / `1` / `2` / `3`: pet size in the panel pill | `auto` |
| `animation_fps` | `4`-`15` (clamped); also scales walking pace | `10` |

## Choose the pet

Available pets:

    cat       orange cat
    puppy     shiba inu puppy
    fox       red fox
    bunny     white bunny
    hamster   golden hamster
    hedgehog  brown hedgehog
    piglet    pink piglet
    duckling  yellow duckling
    frog      green frog
    turtle    green turtle
    gator     the glasskit mascot, drawn in code
    random    a random animal each time the applet starts (default)
    off       plain accent dot (also `0` or `false`)

Where the pet is read from, first match wins: `GLASSKIT_PET=<word>` in the applet's environment
(`GLASSKIT_PET=0` disables), `pet` in `island.conf`, the older one-word file
`~/.config/glasskit/pet` (still supported), then `random`. Names are not case sensitive; an
unknown value is skipped and the next source is used.

## Credits

Animal sprites: Pets by Frakassets (https://frakassets.itch.io/free-pets-pack), CC BY 4.0,
https://creativecommons.org/licenses/by/4.0/. Only the 32px `scroller` `outline` sheets (`lit` +
`shine` layers) and their JSON are bundled, in `assets/pets/`, together with `LICENSE.txt`.
They are embedded in the binary (about 1.2 MB). The gator is original glasskit art.
