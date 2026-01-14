use reqwest::Client;
use serde::Deserialize;
use anyhow::Result;

#[derive(Clone)]
pub struct TmdbClient {
    client: Client,
    api_key: String,
    base_url: String,
}

#[derive(Deserialize, Debug)]
struct ImageResponse {
    posters: Vec<PosterImage>,
}

#[derive(Deserialize, Debug)]
struct PosterImage {
    file_path: String,
    iso_639_1: Option<String>,
    // Ajout des dimensions et des votes pour le tri intelligent
    width: u32,
    height: u32,
    vote_average: f64,
}

#[derive(Deserialize, Debug)]
struct MovieDetails {
    poster_path: Option<String>,
}

impl TmdbClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.themoviedb.org/3".to_string(),
        }
    }

    /// Récupère le MEILLEUR poster textless (le plus haute définition)
    pub async fn get_textless_poster(&self, tmdb_id: &str) -> Result<Option<String>> {
        let url = format!("{}/movie/{}/images?api_key={}", self.base_url, tmdb_id, self.api_key);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { return Ok(None); }

        let images: ImageResponse = resp.json().await?;

        // 1. Filtrer les candidats Textless ("xx" ou null)
        let mut candidates: Vec<&PosterImage> = images.posters.iter()
            .filter(|p| {
                match &p.iso_639_1 {
                    Some(lang) => lang == "xx" || lang == "null",
                    None => true // Pas de langue = textless souvent
                }
            })
            .collect();

        // 2. Si on en a trouvé, on trie pour trouver le "King"
        if !candidates.is_empty() {
            // Tri : D'abord par Résolution (Haut x Large), ensuite par Note (vote_average)
            candidates.sort_by(|a, b| {
                let res_a = a.width * a.height;
                let res_b = b.width * b.height;
                
                // On compare b à a pour avoir un tri DÉCROISSANT (le plus grand en premier)
                res_b.cmp(&res_a)
                    .then(b.vote_average.partial_cmp(&a.vote_average).unwrap_or(std::cmp::Ordering::Equal))
            });

            // On prend le premier (le plus grand)
            if let Some(best) = candidates.first() {
                println!("      ✨ Meilleur poster textless trouvé : {}x{} (Note: {})", best.width, best.height, best.vote_average);
                return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", best.file_path)));
            }
        }

        // 3. Fallback : Si pas de "xx", on cherche le meilleur en Français "fr"
        let mut fr_candidates: Vec<&PosterImage> = images.posters.iter()
            .filter(|p| p.iso_639_1.as_deref() == Some("fr"))
            .collect();

        if !fr_candidates.is_empty() {
            // Même tri : Résolution max
            fr_candidates.sort_by(|a, b| (b.width * b.height).cmp(&(a.width * a.height)));
            
            if let Some(best_fr) = fr_candidates.first() {
                println!("      ⚠️ Pas de textless pur, utilisation du meilleur 'fr' : {}x{}", best_fr.width, best_fr.height);
                return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", best_fr.file_path)));
            }
        }

        Ok(None)
    }

    pub async fn get_standard_poster(&self, tmdb_id: &str) -> Result<Option<String>> {
        let url = format!("{}/movie/{}?api_key={}", self.base_url, tmdb_id, self.api_key);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { return Ok(None); }

        let details: MovieDetails = resp.json().await?;

        if let Some(path) = details.poster_path {
            return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", path)));
        }

        Ok(None)
    }
}