package com.peterphi.unifiprotect.ubv;

import java.time.Instant;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;
import java.util.stream.Stream;

/**
 * Parses the human-readable output of ubnt_ubvinfo. N.B. this is quite a fragile parsing mechanism, since if the tool's output
 * changes it'll be necessary to amend/rewrite this parser. Expects output that looks like this:
 * <pre>Type   TID  KF           OFFSET     SIZE          DTS   CTS              WC     CR
 * ----------- PARTITION START -----------
 *    V     7   1               84   434741      3327378     0 140640421848828  90000       0
 *    A  1000   1           434848      171      1796620     0  75008225008060  48000     459
 *    A  1000   1           435040      171      1797644     0  75008225009084  48000      21
 *    A  1000   1           435232      170      1798668     0  75008225010108  48000      21
 *    A  1000   1           435424      171      1799692     0  75008225011132  48000      22
 *    V     7   0           435616    25698      3333378     0 140640421854828  90000    -456
 *    A  1000   1           461336      171      1800716     0  75008225012156  48000     477
 *    A  1000   1           461528      170      1801740     0  75008225013180  48000      21</pre>
 */
public class UbvInfoParser
{
	private static final String PARTITION_START = "----------- PARTITION START -----------";
	private static final String FIELDVAL_TRACK_TYPE_VIDEO = "V";
	private static final String FIELDVAL_TRACK_TYPE_AUDIO = "A";

	/**
	 * Observed values: V=Video, A=Audio
	 */
	private static final int FIELD_TRACK_TYPE = 1;
	/**
	 * Observed values: 7=Main video, 1000=Main Audio
	 */
	private static final int FIELD_TRACK_ID = 2;
	/**
	 * 1=keyframe (on video tracks).
	 */
	private static final int FIELD_IS_KEYFRAME = 3;
	private static final int FIELD_OFFSET = 4;
	private static final int FIELD_SIZE = 5;
	/**
	 * WC field: wall-clock perhaps? value is UTC time since 1970, expressed in units of FIELD_WC_TBC. Divide by TBC to get
	 * fractional seconds.
	 */
	private static final int FIELD_WC = 8;
	/**
	 * Timebase for track
	 */
	private static final int FIELD_WC_TBC = 9;

	/**
	 * Pre-compiled regex to match runs of spaces; used to split fields in space-separated output of ubnt_ubvinfo
	 */
	private static final Pattern REGEX_SPACES = Pattern.compile(" +");


	/**
	 * Takes the output of ubvinfo, expected to start with the following line:
	 * <pre>Type   TID  KF           OFFSET     SIZE          DTS   CTS              WC     CR</pre>
	 *
	 * @param lines
	 * @return
	 */
	public static List<UbvPartition> parse(Stream<String> lines)
	{
		final List<UbvPartition> results = new ArrayList<>();
		final Map<String, UbvTrack> trackCache = new HashMap<>();

		UbvPartition current = null;
		boolean firstLine = true;

		int videoFrames = 0;

		final Iterator<String> iterator = lines.iterator();
		while (iterator.hasNext())
		{
			final String line = iterator.next();

			if (firstLine)
			{
				// Skip the first line (column headers) explicitly
				firstLine = false;
			}
			else if (line.equals(PARTITION_START))
			{
				// Mark the existing partition as complete (if one exists)
				if (current != null)
					current.complete();

				// Start a new partition
				current = new UbvPartition(results.size() + 1);
				results.add(current);

				videoFrames = 0;
				trackCache.clear();
			}
			else if (Character.isWhitespace(line.charAt(0)))
			{
				final String[] fields = REGEX_SPACES.split(line);

				try
				{
					final UbvTrack track = getTrackForFrame(current, fields, trackCache);

					track.frames++;

					final int offset = Integer.parseInt(fields[FIELD_OFFSET]);
					final int size = Integer.parseInt(fields[FIELD_SIZE]);

					final FrameDataRef frame = new FrameDataRef(track, offset, size);

					// Populate additional timecode data for the first PROBE_FRAMES frames of video
					if (track.frames <= UbvTrack.PROBE_FRAMES)
					{
						frame.wc = Long.parseLong(fields[FIELD_WC]);
						frame.tbc = Integer.parseInt(fields[FIELD_WC_TBC]);

						if (track.startTimecode == null && frame.tbc > 0)
						{
							// N.B. wc/tbc gives UTC seconds since 1970, we want milliseconds
							final long utcMillis = (frame.wc * 1000) / frame.tbc;

							track.startTimecode = Instant.ofEpochMilli(utcMillis); // Convert wc to utc millis
						}
					}

					current.frames.add(frame);
				}
				catch (Throwable t)
				{
					throw new RuntimeException("Error parsing " + Arrays.asList(fields) + ": " + t.getMessage(), t);
				}
			}
		}

		if (current != null)
			current.complete();

		return results;
	}


	/**
	 * Helper method that parses a track structure out of a frame
	 *
	 * @param trackCache
	 * @param fields
	 * @return
	 */
	private static UbvTrack getTrackForFrame(final UbvPartition partition,
	                                         final String[] fields,
	                                         final Map<String, UbvTrack> trackCache)
	{
		UbvTrack track = trackCache.get(fields[FIELD_TRACK_ID]);
		if (track == null)
		{
			final String trackIdStr = fields[FIELD_TRACK_ID];

			final TrackType type = fields[FIELD_TRACK_TYPE].equals(FIELDVAL_TRACK_TYPE_VIDEO) ? TrackType.VIDEO : TrackType.AUDIO;
			final int trackId = Integer.parseInt(trackIdStr);

			track = new UbvTrack(type, trackId);
			trackCache.put(trackIdStr, track);
			partition.tracks.add(track);
		}
		return track;
	}
}
