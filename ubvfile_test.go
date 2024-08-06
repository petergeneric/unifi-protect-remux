package main

import (
	"log"
	"testing"
	"time"
	"ubvremux/ubv"
)

func TestGenerateTimecode(t *testing.T) {
	timecode := ubv.GenerateTimecode(time.Date(2023, time.Month(5), 16, 11, 58, 26, 500000000, time.UTC), 30)
	log.Printf("Timecode Generated")
	if timecode != "11:58:26.16" {
		t.Errorf("Timecode generated is incorrect, got: %s, want: %s.", timecode, "11:58:26.16")
	}
}

func TestCopyFrames(t *testing.T) {
	ubvFile := "samples/FCECDA1F0A63_0_rotating_1597425468956.ubv"

	info := ubv.Analyse(ubvFile, true, ubv.TrackVideo)

	log.Printf("\n\n*** Parsing complete! ***\n\n")
	log.Printf("Number of partitions: %d", len(info.Partitions))

	if len(info.Partitions) > 0 {
		log.Printf("Partition %d", info.Partitions[0].Index)
		log.Printf("Tracks: %d", len(info.Partitions[0].Tracks))
		log.Printf("Frames: %d", len(info.Partitions[0].Frames))

		mainVideoTrack := info.Partitions[0].Tracks[ubv.TrackVideo]
		altVideoTrack := info.Partitions[0].Tracks[ubv.TrackVideoHevcUnknown]
		if mainVideoTrack != nil {
			log.Printf("Start Timecode: %s", mainVideoTrack.StartTimecode.Format(time.RFC3339))
		}
		if altVideoTrack != nil {
			log.Printf("Start Timecode (Alt Video): %s", altVideoTrack.StartTimecode.Format(time.RFC3339))
		}
	}

	//
	//demux.DemuxSinglePartitionToNewFiles(info.Filename, "/tmp/video.h264", "/tmp/audio.aac", info.Partitions[0])

	t.Log("Analysis completed")
}
