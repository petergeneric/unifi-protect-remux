import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.stream.Stream;

public class UbvInfoParser
{
	private static final String PARTITION_START = "----------- PARTITION START -----------";
	private static final int FIELD_OFFSET = 4;
	private static final int FIELD_SIZE = 5;


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

		UbvPartition current = null;
		boolean firstLine = true;

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
				// Start a new partition
				current = new UbvPartition(results.size() + 1);
				results.add(current);
			}
			else if (Character.isWhitespace(line.charAt(0)))
			{
				final String[] fields = line.split(" +", 7);
				final int offset = Integer.parseInt(fields[FIELD_OFFSET]);
				final int size = Integer.parseInt(fields[FIELD_SIZE]);

				current.frames.add(new FrameDataRef(offset, size));
			}
		}

		return results;
	}
}
