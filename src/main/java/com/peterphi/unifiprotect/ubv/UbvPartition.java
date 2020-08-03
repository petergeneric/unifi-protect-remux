package com.peterphi.unifiprotect.ubv;

import java.time.Instant;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;

public class UbvPartition
{
	public final int index;
	public final Set<UbvTrack> tracks = new HashSet<>();
	public final List<FrameDataRef> frames = new ArrayList<>();


	public UbvPartition(final int index)
	{
		this.index = index;
	}


	public void complete()
	{
		for (UbvTrack track : tracks)
		{
			final List<FrameDataRef> ref = frames
					                               .stream()
					                               .filter(f -> f.track == track)
					                               .limit(UbvTrack.PROBE_FRAMES)
					                               .collect(Collectors.toList());

			if (track.type == TrackType.VIDEO && ref.size() >= 2)
			{
				// Ubiquiti always use tbc=90000 for video, so determine framerate based on how much time has elapsed between successive frames
				final FrameDataRef first = ref.get(0);
				final FrameDataRef second = ref.get(1);

				if (first.wc != second.wc)
				{
					// Work out how long (expressed in tbc) has elapsed for this frame/packet
					final int frameTime = (int) (second.wc - first.wc);
					final int millisPerFrame = (frameTime *1000) / second.tbc;

					track.rate = 1000 / millisPerFrame;
				}
			}
			else if (track.type == TrackType.AUDIO && ref.size() >= 1)
			{
				// Ubiquiti use the audio sample rate directly for audio packet tbc
				track.rate = ref.get(0).tbc;
			}
		}
	}


	/**
	 * Get the earliest video track start timecode
	 *
	 * @return the earliest video track start timecode (or null if none available or no video)
	 */
	public Instant getVideoStartTimecode()
	{
		Instant earliest = null;

		for (UbvTrack track : tracks)
		{
			if (track.startTimecode != null && track.type == TrackType.VIDEO)
			{
				if (earliest == null || earliest.isBefore(track.startTimecode))
					earliest = track.startTimecode;
			}
		}

		return earliest;
	}
}
