{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE OverloadedRecordDot #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE StrictData #-}

-- Try on the REPL with
-- (flip patternToEvents (mkArc 0 1)) <$> parseTidal "silence"

module PluguzuCore (
    parseUzu,
    patternToEvents,
    mkArc,
    Event (..),
    encodeEvents,

    -- * re-export
    Pattern.ControlPattern,
    Time.Arc (..),
    Parse.parseTidal,

    -- * presets
    Preset (..),
    readPresets,
    renderPresetData,

    -- * debug
    traceEvent,
)
where

import Control.Applicative ((<|>))
import Control.Monad (forM_, void)
import Data.Aeson (ToJSON (..))
import Data.Aeson qualified as Aeson
import Data.Bits ((.|.))
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.Coerce (coerce)
import Data.Int (Int32)
import Data.List (sortBy)
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe, mapMaybe)
import Data.Word (Word8)
import Debug.Trace (traceShow, traceShowM)
import GHC.Float (double2Float)
import GHC.Generics (Generic)
import Mondo (mondoToTidal)
import Sound.Tidal.Parse qualified as Parse
import Sound.Tidal.Pattern qualified as Pattern
import Sound.Tidal.Show ()
import Sound.Tidal.Time qualified as Time
import Text.Parsec.Error qualified as P
import Text.Parsec.Pos qualified as P

-- import Sound.Tidal.Clock qualified as Clock
-- import Sound.Tidal.Link qualified as Link
-- import Sound.Tidal.Stream.Process qualified as Process

rat2float :: Rational -> Double
rat2float = realToFrac

mkArc :: Rational -> Rational -> Time.Arc
mkArc start stop = Time.Arc start stop

{- | The event generated from pattern.
Note that this needs to be kept in sync with the 'midiEvent' defined in Pluguzu.hpp
-}
data MidiEvent = MidiEvent
    { data0 :: Word8
    , data1 :: Word8
    , data2 :: Word8
    }
    deriving (Show)

data EventLoc = EventLoc
    { col :: Int
    , row :: Int
    , end :: Int
    }
    deriving (Show)

instance ToJSON EventLoc where
    toJSON ev = toJSON [ev.col, ev.row, ev.end]

data EventKind = EventMidi MidiEvent | EventSound String Int Int
    deriving (Show)

data EventSpan = EventSpan
    { start :: Double
    , stop :: Double
    }
    deriving (Show)

data Event = Event
    { span :: EventSpan
    , kind :: EventKind
    , locs :: [EventLoc]
    , value :: Maybe Pattern.ValueMap
    }

instance ToJSON Event where
    toJSON ev = toJSON [span, tag, loc, vals]
      where
        span = toJSON [toJSON ev.span.start, toJSON ev.span.stop]
        tag = case ev.kind of
            EventMidi midi -> toJSON [toJSON midi.data0, toJSON midi.data1, toJSON midi.data2]
            EventSound sound i n -> toJSON [toJSON sound, toJSON i, toJSON n]
        loc = toJSON $ map toJSON ev.locs
        vals = case ev.value of
            Nothing -> Aeson.Null
            Just m -> toJSON $ (fmap encodeValue) $ m

parseUzu :: Bool -> String -> Either (String, Int, Int) Pattern.ControlPattern
parseUzu mondo code
    | mondo = case mondoToTidal code of
        Left err ->
            let pos = P.errorPos err
                message =
                    fmtError . P.showErrorMessages "or" "unknown parse error" "expecting" "unexpected" "end of input" $
                        P.errorMessages err
             in Left (message, P.sourceLine pos, P.sourceColumn pos)
        Right p -> Right p
    | otherwise = case Parse.parseTidal code of
        Left s -> case reads s of
            (line, ':' : rest) : _ | (col, ' ' : msg) : _ <- reads rest -> Left (fmtError msg, line, col)
            _ -> Left (fmtError s, 0, 0)
        Right p -> Right p

fmtError :: String -> String
fmtError ('\n' : xs) = fmtError xs
fmtError xs = go xs
  where
    go [] = []
    go ('\n' : rest) = ' ' : '|' : ' ' : go rest
    go (x : rest) = x : go rest

encodeEvents :: [Event] -> BS.ByteString
encodeEvents = LBS.toStrict . Aeson.encode

patternToEvents :: Pattern.ControlPattern -> Time.Arc -> [Event]
patternToEvents pat arc = sortBy compareEvent $ mapMaybe evmapToEvent $ Pattern.queryArc pat arc

compareEvent :: Event -> Event -> Ordering
compareEvent e1 e2 = compare e1.span.start e2.span.start

evmapToEvent :: Pattern.Event Pattern.ValueMap -> Maybe Event
evmapToEvent ev
    | -- See Sound.Tidal.Stream.Process.
      -- Without this, when playing 's "sax" # cutoff "1 2"'
      -- we would send 'sax' twice.
      Pattern.eventHasOnset ev = do
        (kind, value) <- soundEvent <|> midiEvent
        pure $ Event span kind (map mkLoc ev.context.contextPosition) value
    | otherwise = Nothing
  where
    arc = Pattern.wholeOrPart ev
    start = rat2float (Pattern.start arc)
    stop_ = rat2float (Pattern.stop arc)
    span = EventSpan start stop_
    mkLoc :: ((Int, Int), (Int, Int)) -> EventLoc
    mkLoc ((col, row), (end, _)) = EventLoc col (row - 1) (end - col)
    chan = pred $ max 1 $ min 16 $ round $ fromMaybe 1 $ Pattern.getF =<< Map.lookup "midichan" ev.value
    noteToMidi (pitch, velocity) = MidiEvent (0x90 .|. chan) pitch velocity
    ccToMidi (ccn, ccv) = MidiEvent (0xb0 .|. chan) ccn ccv
    midiEvent = do
        mev <- (ccToMidi <$> eventToCC ev) <|> (noteToMidi <$> eventToNote ev)
        pure (EventMidi mev, Just ev.value)
    soundEvent = do
        s <- Pattern.getS =<< Map.lookup "s" ev.value
        let i = maybe 0 round (Pattern.getN =<< Map.lookup "n" ev.value)
        let note = maybe 36 (round . (+ 72)) (Pattern.getN =<< Map.lookup "note" ev.value)
        let params = Map.filterWithKey (\k _ -> k /= "s" && k /= "n" && k /= "note") ev.value
        pure (EventSound s i note, Just params)

eventToCC :: Pattern.Event Pattern.ValueMap -> Maybe (Word8, Word8)
eventToCC ev = do
    ccn <- Pattern.getF =<< Map.lookup "ccn" ev.value
    ccv <- Pattern.getF =<< Map.lookup "ccv" ev.value
    pure (clamp ccn, clamp ccv)
  where
    clamp = max 0 . min 127 . round

eventToNote :: Pattern.Event Pattern.ValueMap -> Maybe (Word8, Word8)
eventToNote ev = do
    note <- ((+) 72 . round . Pattern.unNote <$> noteEvent) <|> (round . Pattern.unNote <$> midiNoteEvent)
    pure (note, velocity)
  where
    noteEvent = Pattern.getN =<< (Map.lookup "n" ev.value <|> Map.lookup "note" ev.value)
    midiNoteEvent = Pattern.getN =<< Map.lookup "midinote" ev.value

    velocity = fromIntegral $ max 0 $ min 127 $ round $ fromMaybe 100 mVelocity
    mVelocity :: Maybe Double
    mVelocity = getF "velocity" <|> (ampToVel <$> getF "amp") <|> (gainToVel <$> getF "gain")
    getF name = Map.lookup name ev.value >>= Pattern.getF

    -- Based on https://tidalcycles.org/docs/configuration/MIDIOSC/midi/#velocity
    ampToVel amp = 127 * amp
    gainToVel gain = 127 * (gain - 0.3) ** 2.5

firstBar :: Time.Arc
firstBar = Time.Arc 0 4

testPattern :: Pattern.ControlPattern
testPattern = either error id $ Parse.parseTidal "n \"<[c3 f3] [c4 f4]>\""

traceEvent :: Pattern.Event Pattern.ValueMap -> a -> a
traceEvent ev = traceShow (show . encodeValue <$> ev.value)

encodeValue :: Pattern.Value -> Aeson.Value
encodeValue v = case v of
    Pattern.VS str -> toJSON str
    Pattern.VI i -> toJSON i
    Pattern.VF f -> toJSON f
    Pattern.VN num -> toJSON $ num.unNote
    Pattern.VR r -> toJSON r
    Pattern.VB b -> toJSON b
    Pattern.VX xs -> toJSON xs
    Pattern.VPattern pat -> toJSON $ show pat
    Pattern.VState f -> toJSON $ show $ f Map.empty
    Pattern.VList vs -> toJSON $ map encodeValue vs

data Preset = Preset
    { category :: String
    , name :: String
    , lang :: String
    , code :: String
    }
    deriving (Show)

readPresets :: FilePath -> IO [Preset]
readPresets fp = parsePresets "" "" . lines <$> readFile fp
  where
    parsePresets :: String -> String -> [String] -> [Preset]
    parsePresets _ _ [] = []
    parsePresets _ _ (('#' : '#' : ' ' : cat) : xs) = parsePresets cat "" xs
    parsePresets cat _ (('#' : '#' : '#' : ' ' : name) : xs) = parsePresets cat name xs
    parsePresets cat name ("```haskell" : xs) = addPreset cat name "Tidal" xs
    parsePresets cat name ("```mondo" : xs) = addPreset cat name "Mondo" xs
    parsePresets cat name (_ : xs) = parsePresets cat name xs
    addPreset cat name lang xs =
        let (code, rest) = break (== "```") xs
         in Preset cat name lang (unlines code) : parsePresets cat "" rest

renderPresetData :: IO ()
renderPresetData = do
    presets <- readPresets "pluguzu-presets.md"
    writeFile "src/default_presets.rs" $ unlines $ renderHeader presets
  where
    renderHeader presets =
        [ "// Pluguzu presets"
        , ""
        , "use crate::presets::UzuProgram;"
        , "use crate::haskell::UzuKind;"
        , ""
        , "pub fn presets() -> Vec<UzuProgram> {"
        , "  vec!["
        , unlines (map mkProgram presets) ++ "  ]"
        , "}"
        ]
    mkProgram preset = "      UzuProgram {name: " ++ show preset.name ++ ".into(), lang: UzuKind::" ++ preset.lang ++ ", code: " ++ show preset.code ++ ".into(), tags: vec![" ++ show preset.category ++ ".into()]},"
