import com.peterphi.unifiprotect.ubv.TrackType;
import com.peterphi.unifiprotect.ubv.UbvInfoParser;
import com.peterphi.unifiprotect.ubv.UbvPartition;
import com.peterphi.unifiprotect.ubv.UbvTrack;
import org.junit.Test;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.time.Instant;
import java.util.List;
import java.util.stream.Stream;
import java.util.zip.GZIPInputStream;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;

public class UbvInfoParserTest
{
	/**
	 *
	 */
	@Test
	public void testDateTimeParse()
	{
		final Instant instantFromFilename = Instant.ofEpochMilli(1556890741069L);
		final Instant instantFromWcColumn = Instant.ofEpochMilli(1556888562619L);

		assertEquals("2019-05-03T13:39:01.069Z", instantFromFilename.toString());
		assertEquals("2019-05-03T13:02:42.619Z", instantFromWcColumn.toString());
	}


	@Test
	public void testSinglePartitionFile() throws IOException
	{
		final List<UbvPartition> partitions = UbvInfoParser.parse(readUbvInfo("/FCEFFFFFFFFF_0_rotating_1596266504742.ubv.txt.gz"));

		final UbvPartition p = partitions.get(0);
		final UbvTrack firstAudio = p.tracks.stream().filter(t -> t.type == TrackType.AUDIO).findFirst().orElse(null);
		final UbvTrack firstVideo = p.tracks.stream().filter(t -> t.type == TrackType.VIDEO).findFirst().orElse(null);

		assertEquals("expected partition count", 1, partitions.size());
		assertEquals("packets in partition 1", 10238, p.frames.size());
		assertEquals("tracks in partition 1", 2, p.tracks.size());

		assertNotNull("must find video track", firstVideo);
		assertNotNull("must find audio track", firstAudio);

		assertEquals("video framerate", 30, firstVideo.rate);
		assertEquals("audio sample rate", 44100, firstAudio.rate);
		assertEquals("video start timecode", Instant.parse("2020-08-01T07:21:38.364Z"), firstVideo.startTimecode);
		assertEquals("first partition audio start timecode", Instant.parse("2020-08-01T07:21:38.486Z"), firstAudio.startTimecode);
		assertEquals("UbvPartition.getVideoStartTimecode", firstVideo.startTimecode, p.getVideoStartTimecode());
	}


	@Test
	public void testParseMultiPartition() throws IOException
	{
		final List<UbvPartition> partitions = UbvInfoParser.parse(readUbvInfo("/F09FFFFFFFFF_2_timelapse_1556890741069.ubv.txt.gz"));

		assertEquals("expected partition count", 44, partitions.size());
		final UbvPartition p = partitions.get(0);
		assertEquals("frames in partition 1", 366, p.frames.size());
		assertEquals("partition 1 frame 1 offset", 96, p.frames.get(0).offset);
		assertEquals("partition 1 frame 1 size", 2218, p.frames.get(0).size);
	}


	private Stream<String> readUbvInfo(final String resource) throws IOException
	{
		if (resource.endsWith(".gz"))
			return new BufferedReader(new InputStreamReader(new GZIPInputStream(getClass().getResourceAsStream(resource)))).lines();
		else
			return new BufferedReader(new InputStreamReader(getClass().getResourceAsStream(resource))).lines();
	}
}
