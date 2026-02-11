package ubv

import (
	"fmt"
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
	//The track ID; observed values are 7 for the main video, 1003 for some hevc alt video, and 1000 for main audio (AAC)
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

	// For Video tracks, holds a window of rate estimations per-frame
	// This is populated and used to determine Rate
	RateProbeWindow      [32]int
	RateProbeLastFrameWC int64

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
		log.Fatal("Error parsing WC field!", err)
	}
	if tbc, err = strconv.ParseInt(fields[FIELD_WC_TBC], 10, 64); err != nil {
		log.Fatal("Error parsing TBC field!", err)
	}

	// Bail if we encounter a TBC of 0, otherwise we'll have a divide by zeor
	if tbc == 0 {
		log.Fatal("Parsed TBC returned 0! ", tbc, " for line ", line)
	}

	utcMillis := (wc * 1000) / tbc

	utcSecondsPart := utcMillis / 1000
	utcNanosPart := (utcMillis % 1000) * 1000000
	frameTimecode := time.Unix(utcSecondsPart, utcNanosPart)

	// Special-case 1st and 2nd frames (figuring out start timecode and framerate)
	if track.FrameCount == 0 {
		track.StartTimecode = frameTimecode

		if !track.IsVideo {
			// Ubiquiti use the audio sample rate directly for audio packet tbc
			track.Rate = int(tbc)
		} else {
			log.Printf("First Frame: %s", frameTimecode)
			track.RateProbeLastFrameWC = wc
		}
	} else if track.Rate == 0 && track.IsVideo {
		if track.FrameCount < len(track.RateProbeWindow) {
			// Compute rate based on current+last frame time
			track.RateProbeWindow[track.FrameCount] = int(tbc / (wc - track.RateProbeLastFrameWC))
			track.RateProbeLastFrameWC = wc
		} else {
			// Find the most frequent rate in the probe window
			rate := guessVideoRate(track.RateProbeWindow)

			// Pick 75fps as a reasonable maximum rate
			// The Unifi line currently tops out at 55fps on G4 Pro HFR mode
			if rate > 0 && rate < 76 {
				track.Rate = rate

				log.Println("Video Rate Probe: File appears to be", track.Rate, "fps. Use -force-rate if incorrect.")
			} else if rate == 0 {
				log.Println("Video Rate Probe: WARNING probed rate was", rate, "fps. Assuming timelapse file and using 1fps")
				track.Rate = 1
			} else {
				log.Fatal("Video Rate Probe: WARNING probed rate was", rate, "fps. Assuming invalid. Please use -force-rate ## (e.g. -force-rate 25) based on your camera's frame rate")
				panic("Could not determine sensible video framerate based on data stored in .ubv")
			}
		}
	}

	track.LastTimecode = frameTimecode
}

func guessVideoRate(durations [32]int) int {
	var mostFrequent int
	var frequency int

	counts := map[int]int{}
	for _, val := range durations {
		// Only consider positive values
		if val > 0 {
			counts[val]++
			if counts[val] > frequency {
				frequency = counts[val]
				mostFrequent = val
			}
		}
	}

	return mostFrequent
}

/**
 * Generates a timecode string from a StartTimecode object and framerate.
 * The timecode is set as the wall clock time (so a clip starting at 03:45 pm and 13 seconds will have a timestamp of 03:45:13)
 * Additionally, the nanosecond time value is rounded to the nearest frame index based on the framerate,
 * so a 13.50000 second time is frame 16 on a 30 fps clip (frames are indexed from 1 onwards).
 * So the clip will have a full timestamp of 03:34:13.16
 *
 * @param startTimecode The StartTimecode object to generate a timecode string from
 * @param framerate The framerate of the video
 * @return The timecode string
 */
func GenerateTimecode(startTimecode time.Time, framerate int) string {

	var timecode string
	// calculate timecode ( HH:MM:SS.FF ) from seconds and nanoseconds for frame part
	timecode = startTimecode.Format("15:04:05") + "." + fmt.Sprintf("%02.0f", ((float32(startTimecode.Nanosecond())/float32(1000000000.0)*float32(framerate))+1))
	// log.Println("Timecode: ", timecode)
	// log.Printf("Date/Time: %s", videoTrack.StartTimecode)
	return timecode
}
