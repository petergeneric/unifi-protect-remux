package main

import (
	"flag"
	"log"
	"os"
	"path"
	"strconv"
	"strings"
	"time"
	"ubvremux/demux"
	"ubvremux/ffmpegutil"
	"ubvremux/ubv"
)

func main() {
	includeAudioPtr := flag.Bool("audio", false, "If true, extract audio")
	includeVideoPtr := flag.Bool("video", true, "If true, extract video")
	outputFolder := flag.String("outputPath", "./", "The path to output remuxed files to")
	remuxPtr := flag.Bool("remux", true, "If true, remux the resulting bitstreams into an .MP4")

	flag.Parse()

	// Terminate immediately if no .ubv files were provided
	if len(flag.Args()) == 0 {
		println("Expected at least one .ubv file as input!\n")

		flag.Usage()
		os.Exit(1)
	}

	for _, ubvFile := range flag.Args() {
		info := ubv.Analyse(ubvFile)

		log.Printf("\n\n*** Parsing complete! ***\n\n")
		log.Printf("Number of partitions: %d", len(info.Partitions))

		if len(info.Partitions) > 0 {
			log.Printf("First Partition:")
			log.Printf("\tTracks: %d", len(info.Partitions[0].Tracks))
			log.Printf("\tFrames: %d", len(info.Partitions[0].Frames))
			log.Printf("\tStart Timecode: %s", info.Partitions[0].Tracks[7].StartTimecode.Format(time.RFC3339))
		}

		for _, partition := range info.Partitions {
			var videoFile string
			var audioFile string
			var mp4 string
			{
				// TODO generate a base filename that contains the start timecode?
				basename := *outputFolder + "/" + strings.TrimSuffix(path.Base(ubvFile), path.Ext(ubvFile))

				// For multi-partition files, generate a file per partition
				// For single-partition files, we just use the simple output filename
				if len(info.Partitions) > 1 {
					basename = basename + "_p" + strconv.Itoa(partition.Index)
				}

				if *includeVideoPtr && partition.VideoTrackCount > 0 {
					videoFile = basename + ".h264"
				}

				if *includeAudioPtr && partition.AudioTrackCount > 0 {
					audioFile = basename + ".aac"
				}

				if *remuxPtr {
					mp4 = basename + ".mp4"
				}
			}

			demux.DemuxSinglePartitionToNewFiles(ubvFile, videoFile, audioFile, partition)

			if *remuxPtr {
				log.Println("Generating MP4 ", mp4, " from ", videoFile, " and ", audioFile)

				// Spawn FFmpeg to remux
				// TODO: if we do a little parsing of the bitstream we could create an MP4 and reduce write amplification
				ffmpegutil.MuxAudioAndVideo(partition, videoFile, audioFile, mp4)
			}
		}
	}
}
