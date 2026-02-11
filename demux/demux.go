package demux

import (
	"bufio"
	"encoding/binary"
	"io"
	"log"
	"os"
	"ubvremux/ubv"
)

func DemuxSinglePartitionToNewFiles(ubvFilename string, videoFilename string, videoTrackNum int, audioFilename string, partition *ubv.UbvPartition) {

	// The input media file; N.B. we do not use a buffered reader for this because we will be seeking heavily
	ubvFile, err := os.OpenFile(ubvFilename, os.O_RDONLY, 0)
	if err != nil {
		log.Fatal("Error opening UBV file", err)
	}

	defer ubvFile.Close()

	// Optionally write video
	var videoFile *bufio.Writer
	if len(videoFilename) > 0 && partition.VideoTrackCount > 0 {
		videoFileRaw, err := os.Create(videoFilename)
		if err != nil {
			log.Fatal("Error opening video bitstream output", err)
		}

		defer videoFileRaw.Close()
		videoFile = bufio.NewWriter(videoFileRaw)
		defer videoFile.Flush()
	} else {
		videoFile = nil
	}

	// Optionally write audio
	var audioFile *bufio.Writer
	if len(audioFilename) > 0 && partition.AudioTrackCount > 0 {
		audioFileRaw, err := os.Create(audioFilename)
		if err != nil {
			log.Fatal("Error opening audio bitstream output", err)
		}

		defer audioFileRaw.Close()
		audioFile = bufio.NewWriter(audioFileRaw)
		defer audioFile.Flush()
	} else {
		audioFile = nil
	}

	DemuxSinglePartition(ubvFilename, partition, videoFile, videoTrackNum, ubvFile, audioFile)
}

// Extract video and audio data from a given partition of a .ubv file into raw .H264 bitstream and/or raw .AAC bitstream file
func DemuxSinglePartition(ubvFilename string, partition *ubv.UbvPartition, videoFile *bufio.Writer, videoTrackNum int, ubvFile *os.File, audioFile *bufio.Writer) {
	// Allocate a buffer large enough for the largest frame
	var buffer []byte
	{
		bufferSize := 0
		for _, frame := range partition.Frames {
			if frame.Size > bufferSize {
				bufferSize = frame.Size
			}
		}
		buffer = make([]byte, bufferSize)
	}

	// Write opening NAL separator to video track
	if videoFile != nil {
		if bytesWritten, err := videoFile.Write([]byte{0, 0, 0, 1}); err != nil {
			log.Fatal("Failed to write output NAL Separator! Only wrote ", bytesWritten, ". Error:", err)
		} else if bytesWritten != 4 {
			log.Fatal("Tried to write 4 bytes of NAL separator, but wrote ", bytesWritten)
		}
	}

	for _, frame := range partition.Frames {
		if frame.TrackNumber == videoTrackNum && videoFile != nil {
			// Video packet - contains one or more length-prefixed NALs
			frameDataRead := 0

			// N.B. perf of this loop could be improved by simply reading the whole record into
			//      memory and then working on it as a byte array
			for frameDataRead < frame.Size {
				// Seek to H.264 NAL length prefix
				if _, err := ubvFile.Seek(int64(frame.Offset+frameDataRead), io.SeekStart); err != nil {
					log.Fatal("Failed to seek to ", int64(frame.Offset+frameDataRead), " in ", ubvFilename, ": ", err)
				}

				var nalSize int32
				if err := binary.Read(ubvFile, binary.BigEndian, &nalSize); err != nil {
					log.Fatal("Failed to read H.264 NAL size from ", ubvFilename, err)
				} else if frameDataRead+int(nalSize) > frame.Size {
					// Warn if we would read beyond this Frame
					log.Fatal("Read goes beyond frame size! pos within frame: ", frameDataRead, " nalSize: ", nalSize, ", frame.Size:", frame.Size)
				}

				frameDataRead += 4

				// Read
				if _, err := io.ReadFull(ubvFile, buffer[0:nalSize]); err != nil {
					log.Fatal("Failed to read ", frame.Size, " bytes of video essence at ", frame.Offset, err)
				}

				frameDataRead += int(nalSize)

				// Write H.264 essence
				if bytesWritten, err := videoFile.Write(buffer[0:nalSize]); err != nil {
					log.Fatal("Failed to write output video data! Only wrote ", bytesWritten, " bytes. Error:", err)
				}
				// Write NAL separator
				if bytesWritten, err := videoFile.Write([]byte{0, 0, 0, 1}); err != nil {
					log.Fatal("Failed to write output NAL Separator! Only wrote ", bytesWritten, " bytes. Error:", err)
				}
			}

		} else if frame.TrackNumber == ubv.TrackAudio && audioFile != nil {
			// Audio packet - contains raw AAC bitstream

			// Seek
			if _, err := ubvFile.Seek(int64(frame.Offset), io.SeekStart); err != nil {
				log.Fatal("Failed to seek to ", frame.Offset, "in ", ubvFilename, err)
			}

			// Read
			if _, err := io.ReadFull(ubvFile, buffer[0:frame.Size]); err != nil {
				log.Fatal("Failed to read ", frame.Size, " bytes at ", frame.Offset, err)
			}

			if bytesWritten, err := audioFile.Write(buffer[0:frame.Size]); err != nil {
				log.Fatal("Failed to write output audio data! Only wrote ", bytesWritten, ". Error:", err)
			}
		} else {
			continue
		}
	}

	// Flush all buffered output data

	if audioFile != nil {
		audioFile.Flush()
	}

	if videoFile != nil {
		videoFile.Flush()
	}
}
