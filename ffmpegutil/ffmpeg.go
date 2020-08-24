package ffmpegutil

import (
	"log"
	"os"
	"os/exec"
	"strconv"
	"ubvremux/ubv"
)

func MuxVideoOnly(partition *ubv.UbvPartition, h264File string, mp4File string) {
	videoTrack := partition.Tracks[7]

	if videoTrack.FrameCount <= 0 {
		log.Println("Video stream contained zero frames! Skipping this output file: ", mp4File)
		return
	}

	cmd := exec.Command(getFfmpegCommand(), "-i", h264File, "-c", "copy", "-r", strconv.Itoa(videoTrack.Rate), "-y", "-loglevel", "warning", mp4File)

	runFFmpeg(cmd)
}

func MuxAudioOnly(partition *ubv.UbvPartition, aacFile string, mp4File string) {
	cmd := exec.Command(getFfmpegCommand(), "-i", aacFile, "-c", "copy", "-y", "-loglevel", "warning", mp4File)

	runFFmpeg(cmd)
}

func MuxAudioAndVideo(partition *ubv.UbvPartition, h264File string, aacFile string, mp4File string) {
	// If there is no audio file, fall back to the video-only mux operation
	if len(aacFile) <= 0 {
		MuxVideoOnly(partition, h264File, mp4File)
		return
	} else if len(h264File) <= 0 {
		MuxAudioOnly(partition, aacFile, mp4File)
	}

	videoTrack := partition.Tracks[7]
	audioTrack := partition.Tracks[1000]

	if videoTrack.FrameCount <= 0 || audioTrack.FrameCount <= 0 {
		log.Println("Audio/Video stream contained zero frames! Skipping this output file: ", mp4File)
		return
	}

	audioDelaySec := float64(videoTrack.StartTimecode.UnixNano()-audioTrack.StartTimecode.UnixNano()) / 1000000000.0

	cmd := exec.Command(getFfmpegCommand(), "-i", h264File, "-itsoffset", strconv.FormatFloat(audioDelaySec, 'f', -1, 32), "-i", aacFile, "-map", "0:v", "-map", "1:a", "-c", "copy", "-r", strconv.Itoa(videoTrack.Rate), "-y", "-loglevel", "warning", mp4File)

	runFFmpeg(cmd)
}

func runFFmpeg(cmd *exec.Cmd) {
	log.Println("Running: ", cmd.Args)

	// Pass through stdout and stderr
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	err := cmd.Run()
	if err != nil {
		log.Fatal("FFmpeg command failed! Error: ", err)
	}
}

func getFfmpegCommand() string {
	// TODO check on path and then try some defaults
	return "ffmpeg"
}
