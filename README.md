# Hydra Bot

Nothing to see here (for now).

## Developing

### With Rustup

First, install [rustup][1]:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

You should be good to go then.

### With Nix

If you do not have [Nix][2] installed, you can use [Determinate Systems Nix Installer][3] to get it up and running:

```sh
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

Then, you can spawn a shell with all necessary development tools by running this command:

```sh
nix develop
```

## Running

The bot needs a server to connect to, and a .WAD file. You can use the [FreeDOOM's DOOM.WAD][4] or the retail DOOM.WAD that can be found on the game install folder (which you can buy on [Steam][5] or [GOG][6]).

Then, run this command:

```sh
cargo run --release -- -a "<server ip>" -i "<wad file>"
```

[1]: https://rustup.rs
[2]: https://nixos.org
[3]: https://determinate.systems/oss/
[4]: https://freedoom.github.io/download.html
[5]: https://store.steampowered.com/app/2280/DOOM__DOOM_II/
[6]: https://www.gog.com/en/game/doom_doom_ii
