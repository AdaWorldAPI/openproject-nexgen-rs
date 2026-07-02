class Tables::WorkPackages < Tables::Base
  def self.table(migration)
    create_table migration do |t|
      t.string :subject, default: "", null: false
      t.text :description
      t.integer :done_ratio, default: nil, null: true
      t.references :project, null: false
      t.timestamps precision: nil, null: true
    end
  end
end
