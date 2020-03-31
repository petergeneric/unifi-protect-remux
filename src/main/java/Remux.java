import java.io.*;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;

public class Remux
{
	private static final long MAX_UBVINFO_RUNTIME = 120_000;


	public static void main(String[] args) throws Exception
	{
		final File inputFile = new File(args[0]);

		final File parsedFile = new File(inputFile.getParent(), inputFile.getName() + ".txt");

		final List<String> ubvinfoOutput;
		if (parsedFile.exists())
		{
			// ubvinfo parse output already available
			ubvinfoOutput = Files.readAllLines(parsedFile.toPath());
		}
		else
		{
			ubvinfoOutput = ubvinfo(inputFile);
		}

		final int partitions = (int) ubvinfoOutput
				                             .stream()
				                             .filter(s -> s.equals("----------- PARTITION START -----------"))
				                             .count();

		if (partitions != 1)
			throw new IllegalArgumentException(
					"Input file contains multiple partitions, code does not currently handle this (discontinuities). Partition found: " +
					partitions);


		// TODO detect multiple partitions and create multiple output files

		final File h264Stream = new File(inputFile.getParent(), inputFile.getName() + ".h264");

		List<int[]> frames = new ArrayList<>();
		boolean firstLine = true;
		for (String line : ubvinfoOutput)
		{
			if (firstLine)
			{
				firstLine = false;
			}
			else
			{
				if (Character.isWhitespace(line.charAt(0)))
				{
					String[] fields = line.split(" +", 7);

					final int[] data = {Integer.parseInt(fields[4]), Integer.parseInt(fields[5])};
					frames.add(data);
				}
			}
		}

		extractPrimitiveVideoStream(inputFile, h264Stream, frames);
	}


	private static List<String> ubvinfo(final File inputFile) throws IOException, InterruptedException
	{
		// Write ubvinfo to a temporary file
		File tempFile = null;
		try
		{
			tempFile = File.createTempFile("ubv", ".txt");

			ProcessBuilder pb = new ProcessBuilder("ubnt_ubvinfo", "-t", "7", "-P", "-f", inputFile.getAbsolutePath());
			pb.redirectError(new File("/dev/null")); // Discard error
			pb.redirectOutput(tempFile);
			Process process = pb.start();

			long timeout = System.currentTimeMillis() + MAX_UBVINFO_RUNTIME; // Wait 2 minutes at most
			while (process.isAlive() && System.currentTimeMillis() > timeout)
			{
				Thread.sleep(1000);
			}

			if (process.exitValue() != 0)
				throw new IllegalArgumentException("ubvinfo failed!");

			return Files.readAllLines(tempFile.toPath());
		}
		finally
		{
			if (tempFile != null)
				tempFile.delete();
		}
	}


	private static int extractPrimitiveVideoStream(final File inputFile,
	                                               final File h264Stream,
	                                               final List<int[]> frames) throws IOException
	{
		final boolean video = true;

		ByteBuffer buffer = ByteBuffer.allocate(4);

		int nals = 0;

		try (FileInputStream fis = new FileInputStream(inputFile))
		{
			try (FileChannel ic = fis.getChannel())
			{
				try (FileOutputStream fos = new FileOutputStream(h264Stream))
				{
					try (FileChannel oc = fos.getChannel())
					{
						for (int[] offsetAndLength : frames)
						{
							final int frameOffset = offsetAndLength[0];
							final int frameLength = offsetAndLength[1];

							// TODO for video: process NALs (ubv format uses length prefixes rather than NAL separators)
							// TODO for audio: simply extract the raw data
							if (video)
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

									writeNal(oc);


									nals++;


									final long actuallyWritten = ic.transferTo(frameOffset + frameDataRead, nalSize, oc);

									if (actuallyWritten != nalSize)
										throw new IllegalArgumentException("Tried to transfer " +
										                                   nalSize +
										                                   " but only got " +
										                                   actuallyWritten);

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
									throw new IllegalArgumentException("Tried to transfer " +
									                                   frameLength +
									                                   " but only got " +
									                                   actuallyWritten);
							}
						}

						// Write a final NAL Separator
						if (video)
						{
							writeNal(oc);
							nals++;
						}
					}
				}
			}
		}

		return nals;
	}


	private final ByteBuffer nalSeparator = ByteBuffer.allocate(4);

	{
		nalSeparator.mark();
		nalSeparator.put(new byte[]{0, 0, 0, 1});
	}

	private static void writeNal(final FileChannel oc) throws IOException
	{
		oc.write(ByteBuffer.wrap(new byte[]{0, 0, 0, 1}));
	}
}

