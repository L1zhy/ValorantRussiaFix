use curl::easy::{Easy, IpResolve};
use std::io::{self, Write};
use std::time::Duration;

pub fn request(url: &str, timeout: u64, connect_timeout: u64) -> Result<Easy, curl::Error> {
    let mut request = Easy::new();
    request.url(url)?;
    request.proxy("")?;
    request.ip_resolve(IpResolve::V4)?;
    request.connect_timeout(Duration::from_secs(connect_timeout))?;
    request.timeout(Duration::from_secs(timeout))?;
    Ok(request)
}

pub fn download(
    url: &str,
    destination: &mut impl Write,
    max_bytes: u64,
    timeout: u64,
    mut progress: impl FnMut(f64, f64) -> bool,
) -> io::Result<()> {
    let mut request = request(url, timeout, 10).map_err(io::Error::other)?;
    request.fail_on_error(true).map_err(io::Error::other)?;
    request.max_filesize(max_bytes).map_err(io::Error::other)?;
    request.progress(true).map_err(io::Error::other)?;
    let mut written = 0;
    let mut write_result = Ok(());
    let result = {
        let mut transfer = request.transfer();
        transfer
            .progress_function(|total, now, _, _| progress(total, now))
            .map_err(io::Error::other)?;
        transfer
            .write_function(|bytes| {
                written += bytes.len() as u64;
                write_result = if written > max_bytes {
                    Err(io::Error::other("Download exceeds size limit"))
                } else {
                    destination.write_all(bytes)
                };
                Ok(if write_result.is_ok() { bytes.len() } else { 0 })
            })
            .map_err(io::Error::other)?;
        transfer.perform()
    };
    write_result?;
    result.map_err(|error| {
        if error.is_aborted_by_callback() {
            io::Error::new(io::ErrorKind::Interrupted, error)
        } else {
            io::Error::other(error)
        }
    })?;
    if written == 0 || request.response_code().map_err(io::Error::other)? != 200 {
        return Err(io::Error::other("Expected a complete HTTP 200 download"));
    }
    Ok(())
}
