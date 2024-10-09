package main

import (
	"flag"
	"log"
	"os"
	"path"
	"strings"
	"time"
	"ubvremux/demux"
	"ubvremux/ffmpegutil"
	"ubvremux/ubv"
)

// Set at build time (see Makefile) with release tag (for release versions)
var ReleaseVersion string

// Set at build time (see Makefile) with git rev
var GitCommit string

// Parses and validates commandline options and passes them to RemuxCLI
func main() {
	includeAudioPtr := flag.Bool("with-audio", true, "If true, extract audio")
	includeVideoPtr := flag.Bool("with-video", true, "If true, extract video")
	forceRatePtr := flag.Int("force-rate", 0, "If non-zero, adds a -r argument to FFmpeg invocations")
	outputFolder := flag.String("output-folder", "./", "The path to output remuxed files to. \"SRC-FOLDER\" to put alongside .ubv files")
	remuxPtr := flag.Bool("mp4", true, "If true, will create an MP4 as output")
	versionPtr := flag.Bool("version", false, "Display version and quit")
	videoTrackNumPtr := flag.Int("video-track", ubv.TrackVideo, "Video track number to extract (supported: 7, 1003)")

	flag.Parse()

	// Perform some argument combo validation
	if *versionPtr {
		println("UBV Remux Tool")
		println("Copyright (c) Peter Wright 2020-2024")
		println("https://github.com/petergeneric/unifi-protect-remux")
		println("")

		// If there's a release version specified, use that. Otherwise print the git revision
		if len(ReleaseVersion) > 0 {
			println("\tVersion:    ", ReleaseVersion)
		} else {
			println("\tGit commit: ", GitCommit)
		}

		os.Exit(0)
	} else if len(flag.Args()) == 0 {
		// Terminate immediately if no .ubv files were provided
		println("Expected at least one .ubv file as input!\n")

		flag.Usage()
		os.Exit(1)
	} else if !*includeAudioPtr && !*includeVideoPtr {
		// Fail if extracting neither audio nor video
		println("Must enable extraction of at least one of: audio, video!\n")

		flag.Usage()
		os.Exit(1)
	}

	RemuxCLI(flag.Args(), *includeAudioPtr, *includeVideoPtr, *videoTrackNumPtr, *forceRatePtr, *remuxPtr, *outputFolder)
}

// Takes parsed commandline args and performs the remux tasks across the set of input files
func RemuxCLI(files []string, extractAudio bool, extractVideo bool, videoTrackNum int, forceRate int, createMP4 bool, outputFolder string) {
	for _, ubvFile := range files {
		log.Println("Analysing ", ubvFile)
		info := ubv.Analyse(ubvFile, extractAudio, videoTrackNum)

		log.Printf("\n\nAnalysis complete!\n")
		if len(info.Partitions) > 0 {
			log.Printf("First Partition:")
			log.Printf("\tTracks: %d", len(info.Partitions[0].Tracks))
			log.Printf("\tFrames: %d", len(info.Partitions[0].Frames))

			for _, track := range info.Partitions[0].Tracks {
				if track.IsVideo || info.Partitions[0].VideoTrackCount == 0 {
					log.Printf("\tStart Timecode: %s", track.StartTimecode.Format(time.RFC3339))
					break
				}
			}
		}

		log.Printf("\n\nExtracting %d partitions", len(info.Partitions))

		// Optionally apply the user's forced framerate
		if forceRate > 0 {
			log.Println("\nFramerate forced by user instruction: using ", forceRate, " fps")
			for _, partition := range info.Partitions {
				for _, track := range partition.Tracks {
					if track.IsVideo {
						track.Rate = forceRate
					}
				}
			}
		}

		for _, partition := range info.Partitions {
			var videoFile string
			var audioFile string
			var mp4 string
			{
				outputFolder := strings.TrimSuffix(outputFolder, "/")

				if outputFolder == "SRC-FOLDER" {
					outputFolder = path.Dir(info.Filename)
				}

				// Strip the unixtime from the filename, we'll replace with the start timecode of the partition
				baseFilename := strings.TrimSuffix(path.Base(ubvFile), path.Ext(ubvFile))

				// If the filename contains underscores, assume it's a Unifi Protect Filename
				// and drop the final component.
				if strings.Contains(baseFilename, "_") {
					baseFilename = baseFilename[0:strings.LastIndex(baseFilename, "_")]
				}

				basename := outputFolder + "/" + baseFilename + "_" + strings.ReplaceAll(getStartTimecode(partition, videoTrackNum).Format(time.RFC3339), ":", ".")

				if extractVideo && partition.VideoTrackCount > 0 {
					videoFile = basename + ".h264"
				}

				if extractAudio && partition.AudioTrackCount > 0 {
					audioFile = basename + ".aac"
				}

				if createMP4 {
					mp4 = basename + ".mp4"
				}
			}

			// Demux .ubv into .h264 (and optionally .aac) atomic streams
			demux.DemuxSinglePartitionToNewFiles(ubvFile, videoFile, videoTrackNum, audioFile, partition)

			if createMP4 {
				log.Println("\nWriting MP4 ", mp4, "...")

				// Spawn FFmpeg to remux
				ffmpegutil.MuxAudioAndVideo(partition, videoFile, videoTrackNum, audioFile, mp4)

				// Delete
				if len(videoFile) > 0 {
					if err := os.Remove(videoFile); err != nil {
						log.Println("Warning: could not delete ", videoFile+": ", err)
					}
				}
				if len(audioFile) > 0 {
					if err := os.Remove(audioFile); err != nil {
						log.Println("Warning: could not delete ", audioFile+": ", err)
					}
				}
			}
		}
	}
}

func getStartTimecode(partition *ubv.UbvPartition, videoTrackNum int) time.Time {
	for _, track := range partition.Tracks {
		if partition.VideoTrackCount == 0 || (track.IsVideo && track.TrackNumber == videoTrackNum) {
			return track.StartTimecode
		}
	}

	// No start timecode available at all! Return the time of demux as a failsafe
	return time.Now()
}
