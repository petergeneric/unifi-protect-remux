import org.junit.Test;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.util.List;
import java.util.stream.Stream;
import java.util.zip.GZIPInputStream;

import static org.junit.Assert.assertEquals;

public class UbvInfoParserTest
{
	@Test
	public void testSinglePartitionFile() throws IOException
	{
		final List<UbvPartition> partitions = UbvInfoParser.parse(readCompressedInfoFile(
				"/FCECFFFFFFFF_2_rotating_1596209441895.ubv.txt.gz"));

		assertEquals("expected partition count", 1, partitions.size());
		assertEquals("frames in partition 1", 239194, partitions.get(0).frames.size());
	}


	@Test
	public void testParseMultiPartition() throws IOException
	{
		final List<UbvPartition> partitions = UbvInfoParser.parse(readCompressedInfoFile(
				"/F09FFFFFFFFF_2_timelapse_1556890741069.ubv.txt.gz"));

		assertEquals("expected partition count", 44, partitions.size());
		assertEquals("frames in partition 1", 366, partitions.get(0).frames.size());
		assertEquals("partition 1 frame 1 offset", 96, partitions.get(0).frames.get(0).offset);
		assertEquals("partition 1 frame 1 size", 2218, partitions.get(0).frames.get(0).size);
	}


	private Stream<String> readCompressedInfoFile(final String resource) throws IOException
	{
		return new BufferedReader(new InputStreamReader(new GZIPInputStream(getClass().getResourceAsStream(resource)))).lines();
	}
}
