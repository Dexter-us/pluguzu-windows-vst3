{-# LANGUAGE ImportQualifiedPost #-}
{-# LANGUAGE OverloadedRecordDot #-}

module Pluguzu (pluguzuNew, pluguzuParse, pluguzuError) where

import Control.Exception (SomeException, displayException, handle)
import Data.ByteString qualified as BS
import Data.ByteString.Internal qualified as BS
import Data.IORef
import Data.Word (Word64, Word8)
import Foreign (Ptr)
import Foreign qualified
import Foreign.C (CString)
import Foreign.C qualified

-- import System.Posix.Internals (puts)

import PluguzuCore qualified

type CPluguzuState = Foreign.StablePtr PluguzuState

foreign export ccall pluguzuNew :: IO CPluguzuState
foreign export ccall pluguzuParse :: CPluguzuState -> Bool -> Foreign.C.CString -> IO ()
foreign export ccall pluguzuError :: CPluguzuState -> Ptr Word64 -> Ptr Word64 -> IO Foreign.C.CString
foreign export ccall pluguzuRender :: CPluguzuState -> Word64 -> Word64 -> Ptr Word64 -> IO (Foreign.Ptr Word8)

type Error = (CString, (Word64, Word64))

data PluguzuState = PluguzuState
    { error :: IORef (Maybe Error)
    -- ^ The last parse error.
    , pat :: IORef PluguzuCore.ControlPattern
    -- ^ The current pattern
    , events :: IORef BS.ByteString
    -- ^ internal copy of the events to avoid garbage collections
    }

-- | Create a new state.
pluguzuNew :: IO CPluguzuState
pluguzuNew = do
    state <- PluguzuState <$> newIORef Nothing <*> newIORef mempty <*> newIORef mempty
    Foreign.newStablePtr state

writeError :: IORef (Maybe Error) -> Maybe Error -> IO ()
writeError ref new = do
    mPrev <- readIORef ref
    mapM_ (Foreign.free . fst) mPrev
    writeIORef ref new

-- | Parse and load a tidal pattern.
pluguzuParse :: CPluguzuState -> Bool -> Foreign.C.CString -> IO ()
pluguzuParse pState mondo cStr = do
    state <- Foreign.deRefStablePtr pState
    str <- Foreign.C.peekCString cStr
    -- The following pattern raises an error: `n "<0.>"`.
    -- TODO: report the bug upstream: Can't happen, feet are pre-processed.
    let catchException = \(e :: SomeException) -> do
            cerr <- Foreign.C.newCString ("Error: " <> displayException e)
            writeError state.error (Just (cerr, (0, 0)))
    -- putStrLn (PluguzuCore.deltaMini str)
    handle catchException $ case PluguzuCore.parseUzu mondo str of
        Left (err, row, col) -> do
            cerr <- Foreign.C.newCString err
            writeError state.error (Just (cerr, (fromIntegral row, fromIntegral col)))
        Right pat -> do
            writeIORef state.pat $! pat
            writeError state.error Nothing

-- | Get parse error, return NULL when Nothing.
pluguzuError :: CPluguzuState -> Ptr Word64 -> Ptr Word64 -> IO Foreign.C.CString
pluguzuError pState pRow pCol = do
    state <- Foreign.deRefStablePtr pState
    mError <- readIORef state.error
    case mError of
        Nothing -> pure Foreign.nullPtr
        Just (cstr, (row, col)) -> do
            Foreign.poke pRow row
            Foreign.poke pCol col
            pure cstr

pluguzuRender :: CPluguzuState -> Word64 -> Word64 -> Ptr Word64 -> IO (Foreign.Ptr Word8)
pluguzuRender pState startArc endArc pSize = handle catchException $ do
    state <- Foreign.deRefStablePtr pState
    pat <- readIORef state.pat
    let start = fromInteger (toInteger startArc)
    let end = fromInteger (toInteger endArc)
    let arc = PluguzuCore.mkArc start end
    let evs = PluguzuCore.patternToEvents pat arc
    let bs = PluguzuCore.encodeEvents evs
    writeIORef state.events bs
    let BS.BS ptr sz = bs
    Foreign.poke pSize $ fromIntegral sz
    Foreign.withForeignPtr ptr pure
  where
    catchException :: SomeException -> IO (Foreign.Ptr Word8)
    catchException = \(e :: SomeException) -> do
        putStrLn $ "Pluguzu render triggered exception: " <> displayException e
        pure Foreign.nullPtr
