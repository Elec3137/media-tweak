# media-tweak

A GUI to losslessly mess with media files

Currently only packaged with `nix`


# Install

This will compile the program if
it cannot find it in your configured binary caches
```sh
nix profile install github:Elec3137/media-tweak
```

If you have a local copy of the repo you want to use,
you can `cd` into it and do:
```sh
nix profile install .
```

# Develop

The prefered way to develop is with `direnv` (`nix`) and `cargo`;
```sh
git clone https://github.com/Elec3137/media-tweak
cd media-tweak
direnv allow
cargo build # do your thing
```


# TODO

## main goals

1. obsolete the CLI for creating and writing the final output

2. test nix builds with other linux distributions

3. consider looking into macOS support

4. consider looking into windows support

## cosmic

1. fix window decorations (should be able to drag it, close by clicking on the X)

2. fix button and text styles

## *meta*

ordering is by relevance/interest

~50% probablility any one thing will actually happen
