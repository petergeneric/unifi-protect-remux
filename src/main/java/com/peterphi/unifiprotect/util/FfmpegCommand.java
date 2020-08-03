package com.peterphi.unifiprotect.util;

import java.io.File;
import java.io.IOException;
import java.math.BigDecimal;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;

/**
 * Wrapper around FFmpeg
 */
public class FfmpegCommand extends AbstractCommand
{
	/**
	 * Static locations we fall back on if FFmpeg isn't available on PATH
	 */
	private static final File[] STATIC_FFMPEG_LOCATIONS = new File[]{new File("/root/ffmpeg"),
	                                                                 new File("/root/ffmpeg-4.3.1-arm64-static/ffmpeg")};


	@Override
	protected String[] getDefaultCommandExistsCmdline()
	{
		return new String[]{"ffmpeg", "-version"};
	}


	@Override
	protected File[] getStaticLocations()
	{
		return STATIC_FFMPEG_LOCATIONS;
	}


	public void ffmpeg(List<String> args) throws IOException, InterruptedException
	{
		if (!isInstalled())
			throw new RuntimeException("Cannot invoke FFmpeg: does not appear to be installed on the local system");

		final List<String> cmd = new ArrayList<>();
		cmd.add(getBinary());
		cmd.addAll(args);

		final int exitCode = spawnNoOutput(cmd.toArray(new String[args.size()]));

		if (exitCode != 0)
			throw new IllegalArgumentException("FFmpeg operation failed with code " +
			                                   exitCode +
			                                   ". To get error information, re-run command manually: " +
			                                   cmd.stream().collect(Collectors.joining(" ")));
	}


	public void remuxVideoOnly(final File input, final File mp4, final Integer rate) throws IOException, InterruptedException
	{
		// TODO should we request faststart? Would need to estimate the amount of space necessary to reserve for the faststart atom lest we end up burning a lot of IO rewriting the video stream AGAIN

		List<String> args = new ArrayList<>();
		args.add("-i");
		args.add(input.getAbsolutePath());
		args.add("-vcodec");
		args.add("copy");

		if (rate != null)
		{
			args.add("-r");
			args.add(rate.toString());
		}

		args.add("-y");
		args.add(mp4.getAbsolutePath());

		ffmpeg(args);
	}


	public void remuxVideoAndAudio(final File audioFile,
	                               final File h264File,
	                               final Integer rate,
	                               final long audioDelay,
	                               final File mp4) throws IOException, InterruptedException
	{

		List<String> args = new ArrayList<>();

		// Video input
		args.add("-i");
		args.add(h264File.getAbsolutePath());

		// Audio input, along with delay if necessary
		if (audioDelay != 0)
		{
			args.add("-itsoffset");
			args.add("" + BigDecimal.valueOf(((double)audioDelay) / 1000.0).toPlainString());
		}
		args.add("-i");
		args.add(audioFile.getAbsolutePath());

		args.add("-map");
		args.add("0:v");
		args.add("-map");
		args.add("1:a");

		args.add("-c");
		args.add("copy");

		if (rate != null)
		{
			args.add("-r");
			args.add(rate.toString());
		}

		args.add("-y");
		args.add(mp4.getAbsolutePath());

		ffmpeg(args);
	}
}
