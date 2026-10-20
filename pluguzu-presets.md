# The pluguzu presets library

This document contains a library of tidal code for getting started with pluguzu.
After changing the library, run `make render-presets` to update the `src/presets.rs` module. Then rebuild the plugin with `make`.

## Demo

### Haunted

```mondo
$ s ([bd*<2 [2 4]>],
     [~ (sd # n 1 # gain .5)]
) # bank crate
$ s hh&5:12 # n 1 # gain .5 # bank garden
# delay .1 # delaytime .2
$ n <(run 4) ~!3> # s rim # bank crate # gain .4

$ note (perlin # range 0 5 # segment 8)
# scale minPent # ribbon 12 4
# add (note <c3 f3>)
# s sine # dec .5

$ note [c2 ~ c2 c2]
# off 1/3 (add (note 12?))
# jux rev
# s saw # lpf 120 # lpe 2 # dec .9

$ note <~ ~ ~ ~ <c c'min'5 c'min'7>> # s ocarina
# sub (note 12) # gain .03 # hpf 1000 # verb .2

$ note <c5 ~!3> # s sine # verb 1 # vib .25:2
# degrade # gain .1
```

### Gong

```mondo
$ note (perlin # fast 2 # range 0 12 # segment 12)
# scale gong # add (note <c3 f2>) # s sine
# verb [0 0 .5] # sometimes (delay "0.5")
```

### Bass

```mondo
$ note (rand # segment 8 # range 0 5)
# scale minor # ribbon 24 4
# add (note <c2 f1 g1>)
# sometimes (ply 2)
# dec .5 # clip .8
# s [saw,pulse]
# lpf 100 # lpe (sine # range 0 2 # slow 7) # lpq 2
# distort 1
```

### Chime

```mondo
$ (s bd*4, s [~ sd], s hh&5:8 # gain .3) # iter 2
# bank crate # hpf <1!3 [500 1000 1500]>

$ n (rand # segment 8 # range 0 4) # ribbon 16 4
# s large # bank shaker

$ note <[0,2,4] [2,5,7 3] ~>
# off (1/4) (note "0'min" # gain .4)
# scale gong
# add (note <c3@3 f3>)
# iter 2
# s vibraphone
# gain .1

$ note (rand # segment 4 # range 0 5) # ribbon 42 8
# sometimes (ply 2)
# scale gong # sub (note <12 5>)
// # every 4 rev
// # swing .3
# s handchimes
# fast [1 <~ 1.5> 2]
# clip 1
# jux rev
```

### Burst

```mondo
$ s [bd ~ ~  bd:1] # distort <0 [0 1]> # fast 2
$ s [ ~ ~ sd ~] # sometimes (ply 2)
$ s hh&5:8 # n 1 # gain .9 # verb [0 0 .1]
$ note <c3@3 c4> # s piano # hpf 500
$ note [g2!2 <c3 [c3 f4]>]*<4!3 6 8>
# dec (sine # range .2 .4 # slow 2)
# s saw # lpf 200 # lpq 4 # lpe <2 1> # verb .2 # distort .5
```

### Piano

```mondo
$ def foo gong
$ note [<[0,2,4] [5,7,9 5] >,
        (rand # segment 5 # range 12 17)]
# scale foo
# iter 2
# add (note <f3 c3>)
# s piano # clip 2
# hpf 100 # lpf 1500 # pan .8

$ s bd*4 # bank crate
// # jux rev
$ s hh&5:8 # bank garden # gain .4 # iter 2
# sometimes (ply 2) # hpf 200

$ note (rand # segment 8 # range 0 7)
# scale foo
# ribbon 24 3
# add (note <c2@2 g2>) # s saw # lpf 100 # lpe 2
# distort 1 # pan .2 # gain 2
```

### Calm

```haskell
stack [
  n $ scale "minor" $ "{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>"
, ccv (segment 32 $ range 0 128 $ sine) # ccn 1
]
```

### Mondough

```mondo
$ note <d'min9 <g'min7 a'sevenFlat9>>
# clip .8 # release .3
# sub (note 24)
# vib 4:.2
# lpf (sine # range 500 2000 # slow 8)
# lpe 1 # lpd .5 # lpq .1
# s saw # gain .3

$ note <d1 <g1 a1>> # clip .25
# s pulse # pw .4
# lpf 200 # lpe 3 # lpd .1
# off .125 (add (note 12))
# off .25 (add (note 24))
# jux rev
# distort 2:.5
# gain .3
```

## Beat

### 4 on the floor

```haskell
s("bd*4")
```

## Bass

### Funky line

```haskell
n("0 0 - [- 0]")
```

## Melody

### Secret Of Uzu

```haskell
-- todo
```
