package com.peterphi.unifiprotect.remux;

import com.peterphi.unifiprotect.ubv.FrameDataRef;
import com.peterphi.unifiprotect.ubv.TrackType;
import com.peterphi.unifiprotect.ubv.UbvInfoParser;
import com.peterphi.unifiprotect.ubv.UbvPartition;
import com.peterphi.unifiprotect.ubv.UbvTrack;
import com.peterphi.unifiprotect.util.FfmpegCommand;
import com.peterphi.unifiprotect.util.UbvInfoCommand;

import java.io.File;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class Remux
{
	private static final FfmpegCommand FFMPEG = new FfmpegCommand();

	public List<File> files = new ArrayList<>();
	public boolean videoOnly = true;
	public boolean shouldMakeMP4 = true;


	public void parseArguments(String[] args)
	{
		if (args.length == 0)
			args = new String[]{"--help"};

		for (String arg : args)
		{
			if (arg.equals("--help"))
			{
				printHelpText();
				System.exit(0);
			}
			else if (arg.equals("--with-audio"))
			{
				videoOnly = false;
			}
			else if (arg.equals("--raw"))
			{
				shouldMakeMP4 = false;
			}
			else if (arg.startsWith("--"))
			{
				throw new IllegalArgumentException("Unknown argument:  " + arg);
			}
			else
			{
				final File file = new File(arg);

				if (file.exists())
					files.add(file);
				else
					throw new IllegalArgumentException("Supplied .ubv does not exist: " + arg);
			}
		}
	}


	public static void main(String[] args) throws Exception
	{
		final Remux worker = new Remux();

		worker.parseArguments(args);

		if (worker.files.size() == 0)
			throw new IllegalArgumentException("Must provide at least one .ubv input file!");

		// Process each file
		for (File file : worker.files)
			process(worker.videoOnly, worker.shouldMakeMP4, file);
	}


	private static void process(final boolean videoOnly,
	                            final boolean shouldMakeMP4,
	                            final File inputFile) throws IOException, InterruptedException
	{
		System.out.println();
		System.out.println("Processing UBV: " + inputFile);
		if (videoOnly)
			System.out.println("\tVideo Only Mode Enabled");
		if (shouldMakeMP4)
			System.out.println("\tMP4 Mode: will attempt to use FFmpeg to create MP4");
		else
			System.out.println("\tRaw mode: will extract raw bitstreams");

		System.out.println("Analysing UBV frames...");
		final List<UbvPartition> partitions = analyse(inputFile, videoOnly);

		System.out.println("Complete " + partitions.size() + " partitions. Extracting frames...");

		for (UbvPartition partition : partitions)
		{
			final Map<UbvTrack, File> outputs = new HashMap<>();
			{
				for (UbvTrack track : partition.tracks)
				{
					final File outputFile = new File(inputFile.getParent(), getOutputFilename(inputFile, partition, track));

					outputs.put(track, outputFile);
				}
			}

			if (outputs.isEmpty())
			{
				System.err.println("Partition does not have any data we are interested in, skipping...");
			}
			else
			{
				System.out.println("Extracting essence with start timecode " + partition.getVideoStartTimecode() + "...");
				demux(inputFile, partition, outputs);

				// Try to run FFmpeg to generate an MP4
				if (shouldMakeMP4)
				{
					createMP4(videoOnly, outputs);
				}
			}
		}
	}


	private static void createMP4(final boolean videoOnly, final Map<UbvTrack, File> bitstreams)
	{
		if (!FFMPEG.isInstalled())
		{
			System.err.println("FFmpeg is not installed. Leaving raw .h264 and .aac bitstream files for you to process yourself.");
			return;
		}

		if (videoOnly || hasOnlyVideo(bitstreams))
		{
			// We only have a video track, this is an easy remux operation (no AV sync work required)
			for (Map.Entry<UbvTrack, File> entry : bitstreams.entrySet())
			{
				final UbvTrack track = entry.getKey();

				if (track.type == TrackType.VIDEO)
				{
					final File h264File = entry.getValue();
					final File mp4 = new File(h264File.getParentFile(), h264File.getName().replaceAll(".h264", ".mp4"));

					System.out.println("Creating MP4 from video bitstream...");
					try
					{
						FFMPEG.remuxVideoOnly(h264File, mp4, track.getFfmpegRate());

						System.out.println("Complete: created " + mp4);

						// Delete the intermediate .h264 file
						h264File.delete();
					}
					catch (Exception e)
					{
						System.err.println("FFmpeg mux to MP4 failed: " + e.getMessage());
					}
				}
			}
		}
		else
		{
			final Map.Entry<UbvTrack, File> audio = bitstreams
					                                        .entrySet()
					                                        .stream()
					                                        .filter(e -> e.getKey().type == TrackType.AUDIO)
					                                        .findFirst()
					                                        .orElse(null);

			if (audio == null)
				throw new IllegalArgumentException(
						"Unexpectedly found no audio tracks while trying to mux video and audio together!");

			final UbvTrack audioTrack = audio.getKey();
			final File audioFile = audio.getValue();

			boolean anyFailed = false;
			for (Map.Entry<UbvTrack, File> track : bitstreams.entrySet())
			{
				if (track.getKey().type == TrackType.VIDEO)
				{
					final UbvTrack videoTrack = track.getKey();
					final File h264File = track.getValue();

					final File mp4 = new File(h264File.getParentFile(), h264File.getName().replaceAll(".h264", ".mp4"));

					final long audioDelay;
					if (videoTrack.startTimecode != null && audioTrack.startTimecode != null)
						audioDelay = videoTrack.startTimecode.toEpochMilli() - audioTrack.startTimecode.toEpochMilli();
					else
						audioDelay = 0;

					try
					{
						FFMPEG.remuxVideoAndAudio(audioFile, h264File, videoTrack.getFfmpegRate(), audioDelay, mp4);

						// Don't need this video intermediate anymore
						h264File.delete();
					}
					catch (Exception e)
					{
						anyFailed = true;
						System.err.println("FFmpeg mux to MP4 failed: " + e.getMessage());
					}
				}
			}

			// If there were no mux failures, delete all intermediate files
			if (!anyFailed)
			{
				for (File file : bitstreams.values())
				{
					if (file.exists())
						file.delete();
				}
			}
		}
	}


	private static void printHelpText()
	{
		System.out.println("UBV File Remuxer, copyright (c) 2020 Peter Wright");
		System.out.println("Usage: remux [--with-audio] file1.ubv [file2.ubv ...]");
		System.out.println();
		System.out.println("    --with-audio      If supplied, audio will be extracted too");
		System.out.println("    --raw             If supplied, will not attempt to remux the extracted bitstreams");
	}


	/**
	 * Returns true if ALL tracks referenced are of type VIDEO
	 *
	 * @param outputs
	 * @return
	 */
	private static boolean hasOnlyVideo(final Map<UbvTrack, File> outputs)
	{
		return outputs.keySet().stream().allMatch(t -> t.type == TrackType.VIDEO);
	}


	/**
	 * Analyses a .ubv file, returning track & timecode data, as well as a list of frame data positions
	 *
	 * @param videoOnly if true, only the video track will be analysed; used to accelerate a video-only operations
	 * @param inputFile
	 * @return
	 * @throws IOException
	 * @throws InterruptedException
	 */
	private static List<UbvPartition> analyse(final File inputFile,
	                                          final boolean videoOnly) throws IOException, InterruptedException
	{
		// Support a pre-analysed file (means that demux can be run on a different node without ubnt_ubvinfo available)
		final File preAnalysed = new File(inputFile.getParent(), inputFile.getName() + ".txt");

		if (preAnalysed.exists())
		{
			// ubvinfo output already available
			System.out.println("Cached ubnt_ubvinfo is available, using that instead of invoking ubnt_ubvinfo locally");

			final List<UbvPartition> rawData = UbvInfoParser.parse(Files.lines(preAnalysed.toPath()));

			// If a video-only export has been requested, throw away any audio data
			if (videoOnly)
			{
				for (UbvPartition partition : rawData)
				{
					partition.tracks.removeIf(t -> t.type != TrackType.VIDEO);
					partition.frames.removeIf(f -> f.track.type != TrackType.VIDEO);
				}
			}

			return rawData;
		}
		else
		{
			// ubvinfo output not available, so will need local access to the tool
			UbvInfoCommand ubvinfo = new UbvInfoCommand();

			// TODO rely on native format parsing if ubnt_ubvinfo isn't available?
			System.out.println("Invoking ubnt_ubvinfo locally...");

			return ubvinfo.ubvinfo(inputFile, videoOnly);
		}
	}


	/**
	 * Comes up with an output filename based on the input filename
	 *
	 * @param inputFile the source .ubv file
	 * @param partition the partition the track comes from
	 * @param track     the track being extracted into this file
	 * @return a suitable filename
	 */
	private static String getOutputFilename(final File inputFile, final UbvPartition partition, final UbvTrack track)
	{
		final String ext;
		if (track.type.isDefaultTrack(track.id))
			ext = track.type.getDefaultExtension(); // main track of this type
		else
			ext = "-" + track.id + track.type.getDefaultExtension(); // Just in case we encounter multiple tracks of the same typ

		return inputFile.getName() + "." + String.valueOf(track.startTimecode).replaceAll("[:+]", ".") + ext;
	}


	private static void demux(final File inputFile,
	                          final UbvPartition partition,
	                          final Map<UbvTrack, File> trackFileMap) throws IOException
	{
		final Map<UbvTrack, FileChannel> trackToFileChannel = new HashMap<>();
		try
		{
			// Open FileChannels for all the desired outputs
			for (Map.Entry<UbvTrack, File> entry : trackFileMap.entrySet())
			{
				FileChannel fc = FileChannel.open(entry.getValue().toPath(),
				                                  StandardOpenOption.WRITE,
				                                  StandardOpenOption.CREATE,
				                                  StandardOpenOption.TRUNCATE_EXISTING);

				trackToFileChannel.put(entry.getKey(), fc);
			}

			try (FileChannel ic = FileChannel.open(inputFile.toPath(), StandardOpenOption.READ))
			{
				// Buffer used to read 4 bytes field length prefix for video track essence
				final ByteBuffer buffer = ByteBuffer.allocate(4);

				for (FrameDataRef frame : partition.frames)
				{
					final FileChannel oc = trackToFileChannel.get(frame.track);

					// If this frame is from a track we want, copy it across
					if (oc != null)
						copyFrame(ic, buffer, frame, oc);
				}
			}
		}
		finally
		{
			// Finalise and close all the output files
			for (Map.Entry<UbvTrack, FileChannel> entry : trackToFileChannel.entrySet())
			{
				try
				{
					final FileChannel fc = entry.getValue();

					// If this is a video output, write a final NAL separator
					if (entry.getKey().type == TrackType.VIDEO)
					{
						writeNalSeparator(fc);
					}

					fc.close();
				}
				catch (IOException e)
				{
					System.err.println("Error finalising/closing output file! " + e.getMessage());
					e.printStackTrace();
				}
			}
		}
	}


	private static void copyFrame(final FileChannel ic,
	                              final ByteBuffer buffer,
	                              final FrameDataRef frame,
	                              final FileChannel oc) throws IOException
	{
		final int frameOffset = frame.offset;
		final int frameLength = frame.size;

		if (frame.track.type == TrackType.VIDEO)
		{
			int frameDataRead = 0;

			while (frameDataRead < frameLength)
			{
				buffer.clear();
				final int headerRead = ic.read(buffer, frameOffset + frameDataRead);
				if (headerRead == 4)
					frameDataRead += 4;
				else
					throw new IllegalArgumentException("Expected 4 bytes of NAL size, but got " +
					                                   headerRead +
					                                   " bytes! Corrupt media? Reading frame offset " +
					                                   frameOffset +
					                                   " after " +
					                                   frameDataRead +
					                                   " bytes of frame");

				buffer.flip();
				final int nalSize = buffer.getInt();

				writeNalSeparator(oc);


				final long actuallyWritten = ic.transferTo(frameOffset + frameDataRead, nalSize, oc);

				if (actuallyWritten != nalSize)
					throw new IllegalArgumentException("Tried to transfer " + nalSize + " but only got " + actuallyWritten);

				frameDataRead += nalSize;
			}

			if (frameDataRead != frameLength)
				throw new IllegalArgumentException("Read more tham we should from frame! Frame length was " +
				                                   frameLength +
				                                   ", we read " +
				                                   frameDataRead);
		}
		else
		{
			final long actuallyWritten = ic.transferTo(frameOffset, frameLength, oc);

			if (actuallyWritten != frameLength)
				throw new IllegalArgumentException("Tried to transfer " + frameLength + " but only got " + actuallyWritten);
		}
	}


	private static void writeNalSeparator(final FileChannel oc) throws IOException
	{
		oc.write(ByteBuffer.wrap(new byte[]{0, 0, 0, 1}));
	}
}

