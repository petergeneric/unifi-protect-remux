import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.util.List;
import java.util.stream.Stream;

public class Remux
{
	private static final long MAX_UBVINFO_RUNTIME = 120_000;


	public static void main(String[] args) throws Exception
	{
		final File inputFile = new File(args[0]);

		final List<UbvPartition> partitions;
		{
			final File parsedFile = new File(inputFile.getParent(), inputFile.getName() + ".txt");

			if (parsedFile.exists())
			{
				System.out.println("Cached ubnt_ubvinfo is available, using that instead of invoking ubnt_ubvinfo locally");
				// ubvinfo parse output already available
				partitions = UbvInfoParser.parse(Files.lines(parsedFile.toPath()));
			}
			else
			{
				System.out.println("Invoking ubnt_ubvinfo on local machine...");
				Stream<String> ubvinfoOutput = ubvinfo(inputFile);
				partitions = UbvInfoParser.parse(ubvinfoOutput);
			}
		}

		System.out.println("Frame info parsed, extracting video frames...");

		for (UbvPartition partition : partitions)
		{
			final File videoOutput = new File(inputFile.getParent(), getOutputFilename(inputFile, partition));

			System.out.println("Extracting video starting at " + partition.firstFrameTimecode + " to " + videoOutput);

			extractPrimitiveVideoStream(inputFile, videoOutput, partition.frames);
		}
	}


	private static String getOutputFilename(final File inputFile, final UbvPartition partition)
	{
		return inputFile.getName() + "." + partition.firstFrameTimecode.toString().replaceAll("[:+]", ".") + ".h264";
	}


	private static Stream<String> ubvinfo(final File inputFile) throws IOException, InterruptedException
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

			final int exitCode = process.waitFor();

			if (exitCode != 0)
				throw new IllegalArgumentException("ubvinfo failed with code " + exitCode);

			return Files.lines(tempFile.toPath());
		}
		finally
		{
			if (tempFile != null)
				tempFile.delete();
		}
	}


	private static int extractPrimitiveVideoStream(final File inputFile,
	                                               final File h264Stream,
	                                               final List<FrameDataRef> frames) throws IOException
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
						for (FrameDataRef dataref : frames)
						{
							final int frameOffset = dataref.offset;
							final int frameLength = dataref.size;

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

