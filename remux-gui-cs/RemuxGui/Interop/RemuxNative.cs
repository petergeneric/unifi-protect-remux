using System;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.Marshalling;
using System.Text.Json;
using RemuxGui.Models;

namespace RemuxGui.Interop;

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
public delegate void ProgressCallback(IntPtr jsonEvent, int fileIndex);

public static partial class RemuxNative
{
    private const string LibName = "remux_ffi";

    static RemuxNative()
    {
        NativeLibrary.SetDllImportResolver(typeof(RemuxNative).Assembly, ResolveLibrary);
    }

    private static IntPtr ResolveLibrary(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != LibName)
            return IntPtr.Zero;

        string? nativePath = null;

        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            nativePath = System.IO.Path.Combine(AppContext.BaseDirectory, "remux_ffi.dll");
        }
        else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
        {
            nativePath = System.IO.Path.Combine(AppContext.BaseDirectory, "libremux_ffi.dylib");
        }
        else if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
        {
            nativePath = System.IO.Path.Combine(AppContext.BaseDirectory, "libremux_ffi.so");
        }

        if (nativePath != null && NativeLibrary.TryLoad(nativePath, out var handle))
            return handle;

        // Fall back to default resolution
        if (NativeLibrary.TryLoad(libraryName, assembly, searchPath, out handle))
            return handle;

        return IntPtr.Zero;
    }

    [LibraryImport(LibName)]
    private static partial void remux_init();

    [LibraryImport(LibName)]
    private static partial IntPtr remux_version();

    [LibraryImport(LibName)]
    private static partial IntPtr remux_validate_config(
        [MarshalUsing(typeof(Utf8StringMarshaller))] string configJson);

    [LibraryImport(LibName)]
    private static partial IntPtr remux_process_file(
        [MarshalUsing(typeof(Utf8StringMarshaller))] string ubvPath,
        [MarshalUsing(typeof(Utf8StringMarshaller))] string configJson,
        ProgressCallback? progressCallback,
        int fileIndex,
        out IntPtr errorOut);

    [LibraryImport(LibName)]
    private static partial IntPtr remux_produce_diagnostics(
        [MarshalUsing(typeof(Utf8StringMarshaller))] string ubvPath,
        out IntPtr errorOut);

    [LibraryImport(LibName)]
    private static partial int remux_extract_thumbnail(
        [MarshalUsing(typeof(Utf8StringMarshaller))] string mp4Path,
        [MarshalUsing(typeof(Utf8StringMarshaller))] string outputPath,
        uint maxWidth,
        out IntPtr errorOut);

    [LibraryImport(LibName)]
    private static partial void remux_free_string(IntPtr s);

    /// <summary>
    /// Read a UTF-8 string from a native pointer, then free it.
    /// Returns null if the pointer is IntPtr.Zero.
    /// </summary>
    private static string? ReadAndFreeString(IntPtr ptr)
    {
        if (ptr == IntPtr.Zero)
            return null;
        try
        {
            return Marshal.PtrToStringUTF8(ptr);
        }
        finally
        {
            remux_free_string(ptr);
        }
    }

    // --- Public API ---

    public static void Init()
    {
        remux_init();
    }

    public static VersionInfo GetVersion()
    {
        var json = ReadAndFreeString(remux_version());
        if (json == null)
            return new VersionInfo { Version = "unknown" };
        return JsonSerializer.Deserialize(json, AppJsonContext.Default.VersionInfo) ?? new VersionInfo { Version = "unknown" };
    }

    public static (bool valid, string? error) ValidateConfig(RemuxConfig config)
    {
        var configJson = JsonSerializer.Serialize(config, AppJsonContext.Default.RemuxConfig);
        var resultJson = ReadAndFreeString(remux_validate_config(configJson));
        if (resultJson == null)
            return (false, "Internal error: null result from validate_config");

        using var doc = JsonDocument.Parse(resultJson);
        var valid = doc.RootElement.GetProperty("valid").GetBoolean();
        string? error = null;
        if (doc.RootElement.TryGetProperty("error", out var errorProp))
            error = errorProp.GetString();
        return (valid, error);
    }

    public static (string? resultJson, string? error) ProcessFile(
        string ubvPath,
        RemuxConfig config,
        ProgressCallback? callback,
        int fileIndex)
    {
        var configJson = JsonSerializer.Serialize(config, AppJsonContext.Default.RemuxConfig);
        var resultPtr = remux_process_file(ubvPath, configJson, callback, fileIndex, out var errorPtr);
        var error = ReadAndFreeString(errorPtr);
        var result = ReadAndFreeString(resultPtr);
        return (result, error);
    }

    public static string? ExtractThumbnail(string mp4Path, string outputPath, uint maxWidth = 320)
    {
        var ret = remux_extract_thumbnail(mp4Path, outputPath, maxWidth, out var errorPtr);
        var error = ReadAndFreeString(errorPtr);
        return ret == 0 ? null : (error ?? "Unknown error");
    }

    public static (string? outputPath, string? error) ProduceDiagnostics(string ubvPath)
    {
        var resultPtr = remux_produce_diagnostics(ubvPath, out var errorPtr);
        var error = ReadAndFreeString(errorPtr);
        var resultJson = ReadAndFreeString(resultPtr);

        if (resultJson != null)
        {
            using var doc = JsonDocument.Parse(resultJson);
            var outputPath = doc.RootElement.GetProperty("output_path").GetString();
            return (outputPath, null);
        }
        return (null, error ?? "Unknown error");
    }
}
