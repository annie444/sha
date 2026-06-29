use crate::cli::CliAlgorithm;
use sha::algorithm::Algorithm;

impl From<CliAlgorithm> for Algorithm {
    fn from(cli_algo: CliAlgorithm) -> Self {
        type Ca = CliAlgorithm;
        type A = Algorithm;
        match cli_algo {
            Ca::Md5 => A::Md5,
            Ca::Sha1 => A::Sha1,
            Ca::Sha224 => A::Sha224,
            Ca::Sha256 => A::Sha256,
            Ca::Sha384 => A::Sha384,
            Ca::Sha512 => A::Sha512,
            Ca::Sha512_224 => A::Sha512_224,
            Ca::Sha512_256 => A::Sha512_256,
            Ca::Sha3_224 => A::Sha3_224,
            Ca::Sha3_256 => A::Sha3_256,
            Ca::Sha3_384 => A::Sha3_384,
            Ca::Sha3_512 => A::Sha3_512,
        }
    }
}
