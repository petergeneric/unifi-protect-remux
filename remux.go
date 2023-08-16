package main

import (
	"flag"
	"log"
	"os"
	"path"
	"strings"
	"time"
	"ubvremux/demux"
	"ubvremux/ubv"
)

// Set at build time (see Makefile) with release tag (for release versions)
var ReleaseVersion string

// Set at build time (see Makefile) with git rev
var GitCommit string

// Parses and validates commandline options and passes them to RemuxCLI
func main() {
	outputFolder := flag.String("output-folder", "./", "The path to output demuxed files to. \"SRC-FOLDER\" to put alongside .ubv files")
	versionPtr := flag.Bool("version", false, "Display version and quit")

	flag.Parse()

	// Perform some argument combo validation
	if *versionPtr {
		println("UBV Demux Tool")
		println("Copyright (c) Peter Wright 2020-2023")
		println("https://github.com/petergeneric/unifi-protect-remux")
		println("")

		println("\tVersion:    (custom build 1)")
		println("\tGit commit: ", GitCommit)

		os.Exit(0)
	} else if len(flag.Args()) == 0 {
		// Terminate immediately if no .ubv files were provided
		println("Expected at least one .ubv file as input!\n")

		flag.Usage()
		os.Exit(1)
	}

	RemuxCLI(flag.Args(), *outputFolder)
}

// RemuxCLI Takes parsed commandline args and performs the remux tasks across the set of input files
func RemuxCLI(files []string, outputFolder string) {
	for _, ubvFile := range files {
		log.Println("Analysing ", ubvFile)
		info := ubv.Analyse(ubvFile, false)

		log.Printf("\n\nAnalysis complete!")
		log.Printf("Extracting %d partitions...\n", len(info.Partitions))

		for i, partition := range info.Partitions {
			var videoFile string
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

				basename := outputFolder + "/" + baseFilename + "_" + strings.ReplaceAll(getStartTimecode(partition).Format(time.RFC3339), ":", ".")

				videoFile = basename + ".h264"
			}

			log.Printf("Partition %d:", i)
			log.Printf("\tTracks: %d", len(partition.Tracks))
			log.Printf("\tFrames: %d", len(partition.Frames))
			log.Printf("\tStart Timecode: %s", partition.Tracks[7].StartTimecode.Format(time.RFC3339))
			log.Printf("\tOutput File: %s\n", videoFile)

			demux.DemuxSinglePartitionToNewFiles(ubvFile, videoFile, partition)
		}
	}
}

func getStartTimecode(partition *ubv.UbvPartition) time.Time {
	for _, track := range partition.Tracks {
		if partition.VideoTrackCount == 0 || track.IsVideo {
			return track.StartTimecode
		}
	}

	// No start timecode available at all! Return the time of demux as a failsafe
	return time.Now()
}
