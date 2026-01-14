// backend/src/cli.rs
use clap::{Parser, Subcommand};
use dotenv::dotenv;
use std::env;

mod plex;
mod tmdb;
mod image_ops;
mod processor;

use plex::PlexClient;
use tmdb::TmdbClient;

#[derive(Parser)]
#[command(name = "rustizarr")]
#[command(about = "CLI pour gérer les posters Plex (Films + Séries + Saisons)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // ==================== FILMS ====================
    
    /// Lance un scan complet de la bibliothèque FILMS
    Scan {
        #[arg(short, long)]
        library: Option<String>,
        
        /// Forcer le retraitement (ignore le label "Rustizarr")
        #[arg(short, long)]
        force: bool,

        /// Nombre de films à traiter en parallèle (défaut: 1, max: 10)
        #[arg(short, long, default_value = "1")]
        parallel: usize,
    },
    
    /// Traite un seul film par son ID Plex
    Process {
        /// ID du film à traiter
        #[arg(short, long)]
        id: Option<String>,
        
        /// Traiter toute la bibliothèque
        #[arg(short, long)]
        all: bool,
        
        /// Forcer le retraitement (ignore le label "Rustizarr")
        #[arg(short, long)]
        force: bool,
    },
    
    /// Affiche les informations d'un film
    Info {
        #[arg(short, long)]
        id: String,
    },
    
    /// Liste tous les films de la bibliothèque
    List {
        #[arg(short, long)]
        library: Option<String>,
        
        #[arg(long)]
        unprocessed: bool,
    },

    // ==================== SÉRIES ====================
    
    /// Lance un scan complet de la bibliothèque SÉRIES
    ScanShows {
        #[arg(short, long)]
        library: Option<String>,
        
        /// Forcer le retraitement (ignore le label "Rustizarr")
        #[arg(short, long)]
        force: bool,

        /// Nombre de séries à traiter en parallèle (défaut: 1, max: 10)
        #[arg(short, long, default_value = "1")]
        parallel: usize,
    },
    
    /// Traite une seule série par son ID Plex
    ProcessShow {
        /// ID de la série à traiter
        #[arg(short, long)]
        id: String,
        
        /// Forcer le retraitement
        #[arg(short, long)]
        force: bool,
    },
    
    /// Liste toutes les séries de la bibliothèque
    ListShows {
        #[arg(short, long)]
        library: Option<String>,
        
        #[arg(long)]
        unprocessed: bool,
    },

    // ==================== SAISONS ====================
    
    /// Traite toutes les saisons d'une série
    ScanSeasons {
        /// ID Plex de la série
        #[arg(short, long)]
        show_id: String,
        
        /// Forcer le retraitement
        #[arg(short, long)]
        force: bool,
    },
    
    /// Traite une saison spécifique
    ProcessSeason {
        /// ID Plex de la série
        #[arg(short = 's', long)]
        show_id: String,
        
        /// Numéro de la saison
        #[arg(short = 'n', long)]
        season_number: u32,
        
        /// Forcer le retraitement
        #[arg(short, long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    
    let cli = Cli::parse();
    
    let plex_url = env::var("PLEX_URL").expect("❌ PLEX_URL manquant");
    let plex_token = env::var("PLEX_TOKEN").expect("❌ PLEX_TOKEN manquant");
    let tmdb_key = env::var("TMDB_KEY").expect("❌ TMDB_KEY manquant");
    let default_library = env::var("LIBRARY_ID").unwrap_or("1".to_string());
    let default_shows_library = env::var("SHOWS_LIBRARY_ID").unwrap_or("2".to_string());
    
    let plex = PlexClient::new(plex_url, plex_token);
    let tmdb = TmdbClient::new(tmdb_key);
    
    match cli.command {
        // ==================== FILMS ====================
        
        Commands::Scan { library, force, parallel } => {
            let lib_id = library.unwrap_or(default_library);
            let concurrency = parallel.min(10);
            
            if concurrency > 1 {
                println!("🔍 Scan PARALLÈLE de la bibliothèque {} (x{})", lib_id, concurrency);
            } else {
                println!("🔍 Scan séquentiel de la bibliothèque {}", lib_id);
            }
            
            let movie_summaries = plex.get_library_items(&lib_id).await?;
            println!("📚 {} films trouvés", movie_summaries.len());
            
            let mut movies = Vec::new();
            for summary in movie_summaries {
                match plex.get_item_details(&summary.rating_key).await {
                    Ok(movie) => movies.push(movie),
                    Err(e) => println!("⚠️ Erreur pour '{}': {:?}", summary.title, e),
                }
            }
            
            if concurrency > 1 {
                let results = processor::process_library_parallel(&plex, &tmdb, movies, concurrency, force).await;
                
                let mut success = 0;
                let mut skipped = 0;
                let mut errors = 0;
                
                for (title, result) in results {
                    match result {
                        Ok(msg) => {
                            if msg.contains("⏭️") {
                                skipped += 1;
                                println!("⏭️  {}", title);
                            } else {
                                success += 1;
                                println!("✅ {}", title);
                            }
                        },
                        Err(e) => {
                            errors += 1;
                            println!("❌ {} : {:?}", title, e);
                        }
                    }
                }
                
                println!("\n📊 Résumé:");
                println!("   ✅ Succès : {}", success);
                println!("   ⏭️  Ignorés : {}", skipped);
                println!("   ❌ Erreurs : {}", errors);
                
            } else {
                for (index, movie) in movies.iter().enumerate() {
                    println!("\n[{}/{}] {}", index + 1, movies.len(), movie.title);
                    
                    if !force && movie.has_label("Rustizarr") {
                        println!("   ⏭️  Déjà traité");
                        continue;
                    }
                    
                    println!("   ⚙️  Traitement en cours...");
                    
                    match processor::process_movie(&plex, &tmdb, movie.clone()).await {
                        Ok(msg) => println!("   {}", msg),
                        Err(e) => println!("   ❌ Erreur: {:?}", e),
                    }
                }
            }
            
            println!("\n✅ Scan terminé !");
        },
        
        Commands::Process { id, all, force } => {
            if let Some(movie_id) = id {
                println!("⚙️  Traitement du film ID: {}", movie_id);
                
                let movie = plex.get_item_details(&movie_id).await?;
                println!("🎬 Film: {}", movie.title);
                
                if !force && movie.has_label("Rustizarr") {
                    println!("⏭️  Film déjà traité. Utilisez --force pour retraiter.");
                    return Ok(());
                }
                
                if force {
                    println!("🔥 Mode FORCE activé");
                }
                
                match processor::process_movie(&plex, &tmdb, movie).await {
                    Ok(msg) => println!("✅ {}", msg),
                    Err(e) => println!("❌ Erreur: {:?}", e),
                }
                
            } else if all {
                println!("⚙️  Traitement de toute la bibliothèque (force: {})", force);
                let lib_id = env::var("LIBRARY_ID").unwrap_or("1".to_string());
                let movies = plex.get_library_items(&lib_id).await?;
                
                println!("📚 {} films à traiter", movies.len());
                
                for (index, movie_summary) in movies.iter().enumerate() {
                    println!("\n[{}/{}] {}", index + 1, movies.len(), movie_summary.title);
                    
                    match plex.get_item_details(&movie_summary.rating_key).await {
                        Ok(movie) => {
                            if !force && movie.has_label("Rustizarr") {
                                println!("   ⏭️  Déjà traité");
                                continue;
                            }
                            
                            match processor::process_movie(&plex, &tmdb, movie).await {
                                Ok(msg) => println!("   {}", msg),
                                Err(e) => println!("   ❌ Erreur: {:?}", e),
                            }
                        },
                        Err(e) => println!("   ❌ Erreur: {:?}", e),
                    }
                }
                
                println!("\n✅ Traitement terminé !");
                
            } else {
                println!("❌ Erreur: Vous devez spécifier --id ou --all");
            }
        },
        
        Commands::Info { id } => {
            let movie = plex.get_item_details(&id).await?;
            
            println!("\n📽️  Informations du film");
            println!("─────────────────────────────");
            println!("Titre: {}", movie.title);
            println!("Rating Key: {}", movie.rating_key);
            println!("Année: {:?}", movie.year);
            
            if let Some(rating) = movie.audience_rating {
                println!("Score: {:.1}/10", rating);
            }
            
            if movie.has_label("Rustizarr") {
                println!("✅ Déjà traité par Rustizarr");
            } else {
                println!("⏸️  Pas encore traité");
            }
        },
        
        Commands::List { library, unprocessed } => {
            let lib_id = library.unwrap_or(default_library);
            let movies = plex.get_library_items_with_labels(&lib_id).await?;
            
            let filtered: Vec<_> = if unprocessed {
                movies.iter()
                    .filter(|m| !m.has_label("Rustizarr"))
                    .collect()
            } else {
                movies.iter().collect()
            };
            
            println!("\n📋 {} films", filtered.len());
            println!("─────────────────────────────");
            
            for movie in filtered {
                let status = if movie.has_label("Rustizarr") { "✅" } else { "⏸️" };
                println!("{} [{}] {}", status, movie.rating_key, movie.title);
            }
        },

        // ==================== SÉRIES ====================
        
        Commands::ScanShows { library, force, parallel } => {
            let lib_id = library.unwrap_or(default_shows_library);
            let concurrency = parallel.min(10);
            
            if concurrency > 1 {
                println!("📺 Scan PARALLÈLE des séries (bibliothèque {}, x{})", lib_id, concurrency);
            } else {
                println!("📺 Scan séquentiel des séries (bibliothèque {})", lib_id);
            }
            
           let shows = plex.get_shows_library_items(&lib_id).await?;
            println!("📚 {} séries trouvées", shows.len());
            
            if concurrency > 1 {
                let results = processor::process_shows_parallel(&plex, &tmdb, shows, concurrency, force).await;
                
                let mut success = 0;
                let mut skipped = 0;
                let mut errors = 0;
                
                for (title, result) in results {
                    match result {
                        Ok(msg) => {
                            if msg.contains("⏭️") {
                                skipped += 1;
                                println!("⏭️  {}", title);
                            } else {
                                success += 1;
                                println!("✅ {}", title);
                            }
                        },
                        Err(e) => {
                            errors += 1;
                            println!("❌ {} : {:?}", title, e);
                        }
                    }
                }
                
                println!("\n📊 Résumé:");
                println!("   ✅ Succès : {}", success);
                println!("   ⏭️  Ignorés : {}", skipped);
                println!("   ❌ Erreurs : {}", errors);
                
            } else {
                for (index, show) in shows.iter().enumerate() {
                    println!("\n[{}/{}] 📺 {}", index + 1, shows.len(), show.title);
                    
                    if !force && show.has_label("Rustizarr") {
                        println!("   ⏭️  Déjà traitée");
                        continue;
                    }
                    
                    match processor::process_show(&plex, &tmdb, show.clone()).await {
                        Ok(msg) => println!("   {}", msg),
                        Err(e) => println!("   ❌ Erreur: {:?}", e),
                    }
                }
            }
            
            println!("\n✅ Scan des séries terminé !");
        },
        
        Commands::ProcessShow { id, force } => {
            println!("⚙️  Traitement de la série ID: {}", id);
            
            let show = plex.get_show_details(&id).await?;
            println!("📺 Série: {}", show.title);
            
            if !force && show.has_label("Rustizarr") {
                println!("⏭️  Série déjà traitée. Utilisez --force pour retraiter.");
                return Ok(());
            }
            
            if force {
                println!("🔥 Mode FORCE activé");
            }
            
            match processor::process_show(&plex, &tmdb, show).await {
                Ok(msg) => println!("✅ {}", msg),
                Err(e) => println!("❌ Erreur: {:?}", e),
            }
        },
        
        Commands::ListShows { library, unprocessed } => {
            let lib_id = library.unwrap_or(default_shows_library);
            let shows = plex.get_shows_library_items(&lib_id).await?;
            
            let filtered: Vec<_> = if unprocessed {
                shows.iter()
                    .filter(|s| !s.has_label("Rustizarr"))
                    .collect()
            } else {
                shows.iter().collect()
            };
            
            println!("\n📺 {} séries", filtered.len());
            println!("─────────────────────────────");
            
            for show in filtered {
                let status = if show.has_label("Rustizarr") { "✅" } else { "⏸️" };
                println!("{} [{}] {}", status, show.rating_key, show.title);
            }
        },

        // ==================== SAISONS ====================
        
        Commands::ScanSeasons { show_id, force } => {
            println!("🔍 Récupération de la série...");
            let show = plex.get_show_details(&show_id).await?;
            println!("📺 Série: {}", show.title);
            
            let tmdb_id = PlexClient::extract_tmdb_id_from_show(&show)
                .ok_or_else(|| anyhow::anyhow!("Pas d'ID TMDB trouvé pour cette série"))?;
            
            let show_status = tmdb.get_show_status(&tmdb_id).await.ok().flatten();
            
            println!("🔍 Récupération des saisons...");
            let seasons = plex.get_show_seasons(&show_id).await?;
            println!("📚 {} saisons trouvées", seasons.len());
            
            for (index, season) in seasons.iter().enumerate() {
                println!("\n[{}/{}] 📀 Saison {}", index + 1, seasons.len(), season.season_number);
                
                if !force && season.has_label("Rustizarr") {
                    println!("   ⏭️  Déjà traitée");
                    continue;
                }
                
                match processor::process_season(&plex, &tmdb, season.clone(), &tmdb_id, show_status.clone()).await {
                    Ok(msg) => println!("   {}", msg),
                    Err(e) => println!("   ❌ Erreur: {:?}", e),
                }
            }
            
            println!("\n✅ Traitement des saisons terminé !");
        },
        
        Commands::ProcessSeason { show_id, season_number, force } => {
            println!("🔍 Récupération de la série...");
            let show = plex.get_show_details(&show_id).await?;
            println!("📺 Série: {}", show.title);
            
            let tmdb_id = PlexClient::extract_tmdb_id_from_show(&show)
                .ok_or_else(|| anyhow::anyhow!("Pas d'ID TMDB trouvé pour cette série"))?;
            
            let show_status = tmdb.get_show_status(&tmdb_id).await.ok().flatten();
            
            let seasons = plex.get_show_seasons(&show_id).await?;
            let season = seasons.iter()
                .find(|s| s.season_number == season_number)
                .ok_or_else(|| anyhow::anyhow!("Saison {} introuvable", season_number))?;
            
            println!("📀 Traitement de la saison {}", season_number);
            
            if !force && season.has_label("Rustizarr") {
                println!("⏭️  Saison déjà traitée. Utilisez --force pour retraiter.");
                return Ok(());
            }
            
            match processor::process_season(&plex, &tmdb, season.clone(), &tmdb_id, show_status).await {
                Ok(msg) => println!("✅ {}", msg),
                Err(e) => println!("❌ Erreur: {:?}", e),
            }
        },
    }
    
    Ok(())
}
