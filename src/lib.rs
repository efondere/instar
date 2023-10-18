/* use std::{ fs::File };
use flate2::read::GzDecoder;
use tar::Archive;

enum ArchiveError
{
    FileOpenFailure,
}

fn open_archive(archive_path: std::path::PathBuf) -> Result<Archive<GzDecoder<File>>, ArchiveError> {
    let file = match File::open(archive_path) {
        Ok(f) => f,
        Err(_) => return Err(ArchiveError::FileOpenFailure),
    };

    let tar = GzDecoder::new(file);

    Ok(Archive::new(tar))
}

pub fn install(archive_path: std::path::PathBuf)
{

}
*/
