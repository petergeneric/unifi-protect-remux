package main

import (
	"log"
	"testing"
	"time"
	"ubvremux/ubv"
)

func TestCopyFrames(t *testing.T) {
	ubvFile := "samples/FCECDA1F0A63_0_rotating_1597425468956.ubv"

	info := ubv.Analyse(ubvFile, true)

	log.Printf("\n\n*** Parsing complete! ***\n\n")
	log.Printf("Number of partitions: %d", len(info.Partitions))

	if len(info.Partitions) > 0 {
		log.Printf("Partition %d", info.Partitions[0].Index)
		log.Printf("Tracks: %d", len(info.Partitions[0].Tracks))
		log.Printf("Frames: %d", len(info.Partitions[0].Frames))
		log.Printf("Start Timecode: %s", info.Partitions[0].Tracks[7].StartTimecode.Format(time.RFC3339))
	}

	//
	//demux.DemuxSinglePartitionToNewFiles(info.Filename, "/tmp/video.h264", "/tmp/audio.aac", info.Partitions[0])

	t.Log("Analysis completed")
}
