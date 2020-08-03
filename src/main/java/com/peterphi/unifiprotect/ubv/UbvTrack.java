package com.peterphi.unifiprotect.ubv;

import java.time.Instant;

public class UbvTrack
{
	public static final int PROBE_FRAMES = 70;

	public final TrackType type;
	/**
	 * The track ID; only two observed values are 7 for the main video, and 1000 for main audio (AAC)
	 */
	public final int id;

	public Instant startTimecode;

	public int frames = 0;

	/**
	 * The timebase of this content (number of samples every second)
	 */
	public int rate;


	public UbvTrack(final TrackType type, final int id)
	{
		this.type = type;
		this.id = id;
	}


	public Integer getFfmpegRate()
	{
		if (rate <= 0)
			return null;
		else
			return rate;
	}
}
