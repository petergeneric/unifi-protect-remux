package ubv

import (
	"log"
	"strconv"
	"time"
)

const (
	//Observed values: V=Video, A=Audio
	FIELD_TRACK_TYPE = 0

	//Observed values: 7=Main video, 1000=Main Audio
	FIELD_TRACK_ID = 1

	//1=keyframe (on video tracks).
	FIELD_IS_KEYFRAME = 2
	FIELD_OFFSET      = 3
	FIELD_SIZE        = 4

	//WC field: wall-clock perhaps? value is UTC time since 1970, expressed in units of FIELD_WC_TBC. Divide by TBC to get fractional seconds.
	FIELD_WC = 7

	//Timebase for track
	FIELD_WC_TBC = 8

	PROBE_FRAMES = 70
)

type UbvFrame struct {
	//The track ID; only two observed values are 7 for the main video, and 1000 for main audio (AAC)
	TrackNumber int
	Offset      int
	Size        int
}

type UbvTrack struct {
	IsVideo     bool
	TrackNumber int

	// The date+time of the first frame in this partition
	StartTimecode time.Time

	// Number of frames (video) or packets (audio)
	FrameCount int

	// The timebase of this track (number of samples every second)
	// For video, the number of frames per second
	// For audio, the number of samples (N.B. we do not index individual samples)
	Rate int

	// The date+time of the last frame in this partition
	LastTimecode time.Time
}

type UbvPartition struct {
	Index           int
	FrameCount      int
	Tracks          map[int]*UbvTrack
	VideoTrackCount int
	AudioTrackCount int
	Frames          []UbvFrame
}

type UbvFile struct {
	Complete   bool
	Filename   string
	Partitions []*UbvPartition
}

func extractTimecodeAndRate(fields []string, line string, track *UbvTrack) {
	var err error
	var wc int64
	var tbc int64

	if wc, err = strconv.ParseInt(fields[FIELD_WC], 10, 64); err != nil {
		log.Fatal("Error parsing field offset!", err)
	}
	if tbc, err = strconv.ParseInt(fields[FIELD_WC_TBC], 10, 64); err != nil {
		log.Fatal("Error parsing frame size!", err)
	}

	// Bail if we encounter a TBC of 0, otherwise we'll have a divide by zeor
	if tbc == 0 {
		log.Fatal("Parsed TBC returned 0! ", tbc, " for line ", line)
	}

	utcMillis := (wc * 1000) / tbc

	utcSecondsPart := utcMillis / 1000
	utcNanosPart := (utcMillis % 1000) * 1000000
	frameTimecode := time.Unix(utcSecondsPart, utcNanosPart)

	track.LastTimecode = frameTimecode

	// Special-case 1st and 2nd frames (figuring out start timecode and framerate)
	if track.FrameCount == 0 {
		log.Printf("First Frame timestamp %s", frameTimecode)
		track.StartTimecode = frameTimecode

		if !track.IsVideo {
			// Ubiquiti use the audio sample rate directly for audio packet tbc
			track.Rate = int(tbc)
		}
	} else if track.FrameCount == 1 {
		if track.IsVideo {
			log.Printf("Second Frame timestamp %s", frameTimecode)

			// Work out how long (expressed in tbc) has elapsed for this frame/packet
			frameDuration := frameTimecode.Sub(track.StartTimecode)
			track.Rate = int(1000 / frameDuration.Milliseconds())
		}
	}
}
